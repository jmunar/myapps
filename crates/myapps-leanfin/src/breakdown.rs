//! Breakdown: spending inside one label group, over time and by category.
//!
//! Two linked charts share a single payload, so picking a time bucket re-cuts
//! the category chart without another round trip: a time series of the group's
//! total per bucket (day / week / month), and a horizontal bar per category in
//! the group, for the selected bucket or the whole window. Clicking a category
//! bar lists the transactions behind it.

use axum::{
    Extension, Router,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

use super::colors::group_color;
use super::dashboard::leanfin_nav;
use super::labels::{GroupRow, group_display_name, user_groups};
use super::services::expenses::{self, ExpensePoint};
use myapps_core::auth::UserId;
use myapps_core::components::html_escape;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/breakdown", get(page))
        .route("/breakdown/chart", get(chart_data))
        // The tab was called Expenses until the group breakdown replaced it.
        .route("/expenses", get(legacy_redirect))
}

async fn legacy_redirect(state: axum::extract::State<AppState>) -> impl IntoResponse {
    Redirect::permanent(&format!("{}/leanfin/breakdown", state.config.base_path))
}

/// Serialize for embedding in a `<script>` body. `serde_json` handles quotes and
/// control characters; escaping the three markup characters on top of that keeps
/// a category named `</script>` from closing the element.
fn json_for_script(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let label_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leanfin_labels WHERE user_id = ?")
            .bind(user_id.0)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    if label_count == 0 {
        let body = format!(
            r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="empty-state"><p>{no_labels}</p></div>
        </div>"#,
            title = t.exp_title,
            subtitle = t.exp_subtitle,
            no_labels = t.exp_no_labels,
        );
        return Html(render_page(
            &format!("LeanFin — {}", t.expenses),
            &leanfin_nav(base, "breakdown", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let groups: Vec<GroupRow> = user_groups(&state.pool, user_id.0)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        });

    // Exactly one group at a time: these behave as radio buttons, not toggles.
    let mut group_pills = String::new();
    for g in &groups {
        let color = group_color(g.id);
        group_pills.push_str(&format!(
            r##"<button type="button" class="label-badge label-pill" style="--label-color:{color}"
                    data-group-id="{id}" onclick="selectGroup(this)">{name}</button> "##,
            id = g.id,
            name = html_escape(&group_display_name(&g.name, lang)),
        ));
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="expenses-controls" id="breakdown-controls">
                <div class="expenses-labels">
                    {group_pills}
                </div>
                <div class="period-selector">
                    <button type="button" class="period-btn" data-days="30"
                            onclick="selectBreakdownPeriod(this, 30)">30d</button>
                    <button type="button" class="period-btn period-btn-active" data-days="90"
                            onclick="selectBreakdownPeriod(this, 90)">90d</button>
                    <button type="button" class="period-btn" data-days="180"
                            onclick="selectBreakdownPeriod(this, 180)">180d</button>
                    <button type="button" class="period-btn" data-days="365"
                            onclick="selectBreakdownPeriod(this, 365)">365d</button>
                </div>
            </div>
            <div class="chart-container" id="breakdown-chart-container" style="display:none"><canvas id="breakdown-canvas"></canvas></div>
            <div id="breakdown-empty" class="empty-state"><p>{select_group}</p></div>
            <div id="breakdown-data"></div>
        </div>
        <div class="card mt-2" id="breakdown-categories-card" style="display:none">
            <div class="card-header">
                <h2>{by_category}</h2>
                <span id="breakdown-range" class="text-sm text-secondary"></span>
            </div>
            <div class="chart-container" id="breakdown-categories-container">
                <canvas id="breakdown-categories-canvas"></canvas>
            </div>
        </div>
        <div class="card mt-2" id="breakdown-txn-card" style="display:none">
            <div class="card-header">
                <h2>{transactions}</h2>
                <span id="breakdown-txn-range" class="text-sm text-secondary"></span>
            </div>
            <div id="breakdown-txn-table"></div>
        </div>
        <script>
        (function() {{
            var basePath = '{base}';
            var selectedGroup = null;
            var currentDays = 90;
            var timeChart = null;
            var catChart = null;
            var payload = null;
            var selectedBucket = null;   // index into payload.dates, or null for the whole window
            var selectGroupMsg = {select_group_js};
            var allRangeMsg = {all_range_js};

            function addDay(date) {{
                var d = new Date(date + 'T00:00:00Z');
                d.setUTCDate(d.getUTCDate() + 1);
                return d.toISOString().slice(0, 10);
            }}

            // A plotted point is the END of its bucket, so the period behind it
            // starts the day after the previous point.
            function periodFor(index) {{
                var to = payload.dates[index];
                var from = index > 0 ? addDay(payload.dates[index - 1]) : payload.windowStart;
                if (from > to) from = to;
                return [from, to];
            }}

            function activeRange() {{
                if (selectedBucket === null) {{
                    return [payload.windowStart, payload.dates[payload.dates.length - 1]];
                }}
                return periodFor(selectedBucket);
            }}

            function totalsPerBucket() {{
                return payload.dates.map(function(_, i) {{
                    return payload.matrix.reduce(function(sum, row) {{ return sum + row[i]; }}, 0);
                }});
            }}

            // Category totals over the active range, largest first — a bar chart
            // that is not sorted makes the reader do the ranking.
            function categoryTotals() {{
                var from = selectedBucket === null ? 0 : selectedBucket;
                var to = selectedBucket === null ? payload.dates.length - 1 : selectedBucket;
                return payload.categories.map(function(cat, ci) {{
                    var sum = 0;
                    for (var i = from; i <= to; i++) sum += payload.matrix[ci][i];
                    return {{ id: cat.id, name: cat.name, total: sum }};
                }}).filter(function(c) {{ return Math.abs(c.total) > 0.005; }})
                  .sort(function(a, b) {{ return b.total - a.total; }});
            }}

            function money(v) {{
                return v.toLocaleString(undefined, {{minimumFractionDigits: 2, maximumFractionDigits: 2}});
            }}

            function renderTimeChart() {{
                var canvas = document.getElementById('breakdown-canvas');
                var emptyEl = document.getElementById('breakdown-empty');
                canvas.style.display = '';
                canvas.parentElement.style.display = '';
                emptyEl.style.display = 'none';

                var data = {{
                    labels: payload.dates,
                    datasets: [{{
                        label: payload.groupName,
                        data: totalsPerBucket(),
                        backgroundColor: payload.color,
                        borderRadius: 4,
                        borderSkipped: false
                    }}]
                }};
                var options = {{
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {{
                        legend: {{ display: false }},
                        tooltip: {{ callbacks: {{ label: function(ctx) {{ return money(ctx.parsed.y); }} }} }}
                    }},
                    scales: {{
                        x: {{ ticks: {{ maxRotation: 45, font: {{ size: 11 }} }}, grid: {{ display: false }} }},
                        y: {{ ticks: {{ callback: function(v) {{ return v.toLocaleString(); }} }} }}
                    }},
                    onClick: function(evt, elems) {{
                        if (elems.length === 0) return;
                        var i = elems[0].index;
                        selectedBucket = (selectedBucket === i) ? null : i;
                        renderCategoryChart();
                    }}
                }};

                if (timeChart) {{
                    timeChart.data = data;
                    timeChart.options = options;
                    timeChart.update();
                }} else {{
                    timeChart = new Chart(canvas, {{ type: 'bar', data: data, options: options }});
                }}
            }}

            function renderCategoryChart() {{
                var totals = categoryTotals();
                var card = document.getElementById('breakdown-categories-card');
                var range = activeRange();
                document.getElementById('breakdown-range').textContent =
                    selectedBucket === null ? allRangeMsg : range[0] + ' → ' + range[1];

                if (totals.length === 0) {{
                    card.style.display = 'none';
                    if (catChart) {{ catChart.destroy(); catChart = null; }}
                    return;
                }}
                card.style.display = '';

                // One bar per category — keep the plot tall enough that the
                // labels never collide.
                document.getElementById('breakdown-categories-container').style.height =
                    Math.max(160, 44 * totals.length + 60) + 'px';

                var data = {{
                    labels: totals.map(function(c) {{ return c.name; }}),
                    datasets: [{{
                        data: totals.map(function(c) {{ return c.total; }}),
                        backgroundColor: payload.color,
                        borderRadius: 4,
                        borderSkipped: false
                    }}]
                }};
                var options = {{
                    indexAxis: 'y',
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {{
                        legend: {{ display: false }},
                        tooltip: {{ callbacks: {{ label: function(ctx) {{ return money(ctx.parsed.x); }} }} }}
                    }},
                    scales: {{
                        x: {{ ticks: {{ callback: function(v) {{ return v.toLocaleString(); }} }} }},
                        y: {{ grid: {{ display: false }} }}
                    }},
                    onClick: function(evt, elems) {{
                        if (elems.length === 0) return;
                        var cat = totals[elems[0].index];
                        var r = activeRange();
                        loadTransactions(cat.id, cat.name, r[0], r[1]);
                    }}
                }};

                if (catChart) {{
                    catChart.data = data;
                    catChart.options = options;
                    catChart.update();
                }} else {{
                    catChart = new Chart(document.getElementById('breakdown-categories-canvas'),
                                         {{ type: 'bar', data: data, options: options }});
                }}
            }}

            window.updateBreakdown = function(next) {{
                payload = next;
                selectedBucket = null;
                document.getElementById('breakdown-txn-card').style.display = 'none';
                renderTimeChart();
                renderCategoryChart();
            }};

            window.showBreakdownEmpty = function(msg) {{
                payload = null;
                selectedBucket = null;
                var canvas = document.getElementById('breakdown-canvas');
                canvas.style.display = 'none';
                // Hide the container too: its fixed height would otherwise leave
                // 300px of blank space above the message.
                canvas.parentElement.style.display = 'none';
                document.getElementById('breakdown-categories-card').style.display = 'none';
                document.getElementById('breakdown-txn-card').style.display = 'none';
                if (timeChart) {{ timeChart.destroy(); timeChart = null; }}
                if (catChart) {{ catChart.destroy(); catChart = null; }}
                var el = document.getElementById('breakdown-empty');
                el.innerHTML = '<p></p>';
                el.firstChild.textContent = msg;
                el.style.display = '';
            }};

            window.selectGroup = function(btn) {{
                var id = btn.dataset.groupId;
                document.querySelectorAll('#breakdown-controls .label-pill')
                    .forEach(function(b) {{ b.classList.remove('label-pill-active'); }});
                if (selectedGroup === id) {{
                    selectedGroup = null;
                    window.showBreakdownEmpty(selectGroupMsg);
                    return;
                }}
                selectedGroup = id;
                btn.classList.add('label-pill-active');
                loadChart();
            }};

            window.selectBreakdownPeriod = function(btn, days) {{
                document.querySelectorAll('#breakdown-controls .period-btn')
                    .forEach(function(b) {{ b.classList.remove('period-btn-active'); }});
                btn.classList.add('period-btn-active');
                currentDays = days;
                if (selectedGroup) loadChart();
            }};

            function loadChart() {{
                htmx.ajax('GET', basePath + '/leanfin/breakdown/chart?group_id=' + selectedGroup
                          + '&days=' + currentDays, '#breakdown-data');
            }}

            function loadTransactions(labelId, labelName, dateFrom, dateTo) {{
                var url = basePath + '/leanfin/transactions?label_ids=' + labelId
                        + '&date_from=' + dateFrom + '&date_to=' + dateTo;
                document.getElementById('breakdown-txn-card').style.display = '';
                document.getElementById('breakdown-txn-range').textContent =
                    labelName + ' · ' + dateFrom + ' → ' + dateTo;
                htmx.ajax('GET', url, '#breakdown-txn-table');
            }}
        }})();
        </script>"##,
        title = t.exp_title,
        subtitle = t.exp_subtitle,
        select_group = t.exp_select_group,
        by_category = t.exp_by_category,
        transactions = t.exp_transactions,
        select_group_js = json_for_script(&serde_json::Value::from(t.exp_select_group)),
        all_range_js = json_for_script(&serde_json::Value::from(t.exp_full_range)),
    );

    Html(render_page(
        &format!("LeanFin — {}", t.expenses),
        &leanfin_nav(base, "breakdown", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct ChartQuery {
    group_id: i64,
    #[serde(default = "default_days")]
    days: i64,
}

fn default_days() -> i64 {
    90
}

/// Downsample expense data points to weekly or monthly intervals.
/// For expenses, amounts within each interval are summed. The date used is the
/// canonical end of the interval (Sunday for weekly, last day of month for monthly).
fn downsample_expenses(series: &[ExpensePoint], days: i64) -> Vec<ExpensePoint> {
    if days <= 30 || series.is_empty() {
        return series.to_vec();
    }

    // Key: bucket identifier + label_id → aggregated point
    // For weekly: (iso_year, iso_week, label_id)
    // For monthly: (year, month, label_id)
    let mut buckets: BTreeMap<(i32, u32, i64), ExpensePoint> = BTreeMap::new();

    for p in series {
        let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") else {
            continue;
        };
        let (key, bucket_end) = if days <= 90 {
            let key = (d.iso_week().year(), d.iso_week().week(), p.label_id);
            // Sunday = end of ISO week (Mon=0 .. Sun=6)
            let days_until_sunday = (6 - d.weekday().num_days_from_monday()) % 7;
            let end = d + chrono::Duration::days(days_until_sunday as i64);
            (key, end)
        } else {
            let key = (d.year(), d.month(), p.label_id);
            // Last day of the month
            let end = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
            } - chrono::Duration::days(1);
            (key, end)
        };
        let end_str = bucket_end.format("%Y-%m-%d").to_string();
        buckets
            .entry(key)
            .and_modify(|e| {
                e.total += p.total;
            })
            .or_insert_with(|| {
                let mut pt = p.clone();
                pt.date = end_str;
                pt
            });
    }

    buckets.into_values().collect()
}

/// Generate all time window end-dates covering the last `days` days.
/// Daily for <=30d, weekly (ending Sunday) for <=90d, monthly (last day) for longer.
fn generate_window_dates(days: i64) -> Vec<String> {
    let today = chrono::Utc::now().date_naive();
    let start = today - chrono::Duration::days(days);
    let mut dates = Vec::new();

    if days <= 30 {
        // Daily: every day from start to today
        let mut d = start;
        while d <= today {
            dates.push(d.format("%Y-%m-%d").to_string());
            d += chrono::Duration::days(1);
        }
    } else if days <= 90 {
        // Weekly: find the first Sunday on or after start, then every 7 days.
        // Include the current (possibly incomplete) week's Sunday even if after today.
        let days_until_sunday = (6 - start.weekday().num_days_from_monday()) % 7;
        let mut d = start + chrono::Duration::days(days_until_sunday as i64);
        let end_sunday = {
            let dts = (6 - today.weekday().num_days_from_monday()) % 7;
            today + chrono::Duration::days(dts as i64)
        };
        while d <= end_sunday {
            dates.push(d.format("%Y-%m-%d").to_string());
            d += chrono::Duration::days(7);
        }
    } else {
        // Monthly: last day of each month from start's month to today's month
        let mut year = start.year();
        let mut month = start.month();
        loop {
            let last_day = if month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
            } - chrono::Duration::days(1);
            dates.push(last_day.format("%Y-%m-%d").to_string());
            if year > today.year() || (year == today.year() && month >= today.month()) {
                break;
            }
            month += 1;
            if month > 12 {
                month = 1;
                year += 1;
            }
        }
    }

    dates
}

async fn chart_data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<ChartQuery>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    let group: Option<GroupRow> = sqlx::query_as(
        "SELECT id, name, is_default FROM leanfin_label_groups WHERE id = ? AND user_id = ?",
    )
    .bind(params.group_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(group) = group else {
        return Html(empty_script(t.exp_group_not_found));
    };

    let labels: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM leanfin_labels WHERE user_id = ? AND group_id = ? ORDER BY name COLLATE NOCASE",
    )
    .bind(user_id.0)
    .bind(group.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if labels.is_empty() {
        return Html(empty_script(t.exp_group_empty));
    }

    let label_ids: Vec<i64> = labels.iter().map(|(id, _)| *id).collect();
    let raw = expenses::get_expense_series(&state.pool, user_id.0, &label_ids, params.days)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        });
    let series = downsample_expenses(&raw, params.days);

    if series.is_empty() {
        return Html(empty_script(t.exp_no_data));
    }

    let all_dates = generate_window_dates(params.days);

    // (date, label) → total, so the matrix below is a plain lookup.
    let mut data_map: HashMap<(&str, i64), f64> = HashMap::new();
    for p in &series {
        *data_map.entry((p.date.as_str(), p.label_id)).or_default() += p.total;
    }

    let matrix: Vec<Vec<f64>> = label_ids
        .iter()
        .map(|lid| {
            all_dates
                .iter()
                .map(|d| data_map.get(&(d.as_str(), *lid)).copied().unwrap_or(0.0))
                .collect()
        })
        .collect();

    let window_start = (chrono::Utc::now() - chrono::Duration::days(params.days))
        .format("%Y-%m-%d")
        .to_string();

    let payload = serde_json::json!({
        "groupId": group.id,
        "groupName": group_display_name(&group.name, lang),
        "color": group_color(group.id),
        "dates": all_dates,
        "windowStart": window_start,
        "categories": labels
            .iter()
            .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
            .collect::<Vec<_>>(),
        "matrix": matrix,
    });

    Html(format!(
        "<script>window.updateBreakdown({});</script>",
        json_for_script(&payload)
    ))
}

fn empty_script(message: &str) -> String {
    format!(
        "<script>window.showBreakdownEmpty({});</script>",
        json_for_script(&serde_json::Value::from(message))
    )
}
