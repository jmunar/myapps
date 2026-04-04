use axum::{Extension, Router, response::Html, routing::get};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::dashboard::leanfin_nav;
use super::services::expenses::{self, ExpensePoint};
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/expenses", get(page))
        .route("/expenses/chart", get(chart_data))
}

#[derive(sqlx::FromRow)]
struct LabelOption {
    id: i64,
    name: String,
    color: Option<String>,
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let labels: Vec<LabelOption> = sqlx::query_as(
        "SELECT id, name, color FROM leanfin_labels WHERE user_id = ? ORDER BY name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if labels.is_empty() {
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
            &leanfin_nav(base, "expenses", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let mut label_pills = String::new();
    for l in &labels {
        let color = l.color.as_deref().unwrap_or("#6B6B6B");
        label_pills.push_str(&format!(
            r##"<button type="button" class="label-badge label-pill" style="--label-color:{color}"
                    data-label-id="{id}" onclick="toggleLabel(this)">{name}</button> "##,
            id = l.id,
            name = l.name,
        ));
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="expenses-controls" id="expenses-controls">
                <div class="expenses-labels">
                    {label_pills}
                </div>
                <div class="period-selector">
                    <button type="button" class="period-btn" data-days="30"
                            onclick="selectExpensePeriod(this, 30)">30d</button>
                    <button type="button" class="period-btn period-btn-active" data-days="90"
                            onclick="selectExpensePeriod(this, 90)">90d</button>
                    <button type="button" class="period-btn" data-days="180"
                            onclick="selectExpensePeriod(this, 180)">180d</button>
                    <button type="button" class="period-btn" data-days="365"
                            onclick="selectExpensePeriod(this, 365)">365d</button>
                </div>
            </div>
            <div class="chart-container"><canvas id="expenses-canvas" style="display:none"></canvas></div>
            <div id="expenses-empty" class="empty-state"><p>{select_labels}</p></div>
            <div id="expenses-data"></div>
        </div>
        <div class="card mt-2" id="expenses-txn-card" style="display:none">
            <div class="card-header">
                <h2>{transactions}</h2>
                <span id="expenses-txn-range" class="text-sm text-secondary"></span>
            </div>
            <div id="expenses-txn-table"></div>
        </div>
        <script>
        (function() {{
            var basePath = '{base}';
            var selectedLabels = new Set();
            var currentDays = 90;
            var expensesChart = null;
            var selectLabelsMsg = '{select_labels_js}';

            function windowStart(endDate) {{
                var d = new Date(endDate + 'T00:00:00');
                if (currentDays <= 30) return endDate;
                if (currentDays <= 90) {{
                    d.setDate(d.getDate() - 6);
                }} else {{
                    d.setDate(1);
                }}
                return d.toISOString().slice(0, 10);
            }}

            function onBarClick(dates, labelIds) {{
                return function(evt, elems) {{
                    if (elems.length > 0) {{
                        var end = dates[elems[0].index];
                        window.loadTransactions(labelIds, windowStart(end), end);
                    }}
                }};
            }}

            window.updateExpensesChart = function(dates, datasets, labelIds) {{
                var canvas = document.getElementById('expenses-canvas');
                var emptyEl = document.getElementById('expenses-empty');
                canvas.style.display = '';
                emptyEl.style.display = 'none';
                if (expensesChart) {{
                    expensesChart.data.labels = dates;
                    expensesChart.data.datasets = datasets;
                    expensesChart.options.onClick = onBarClick(dates, labelIds);
                    expensesChart.update();
                }} else {{
                    expensesChart = new Chart(canvas, {{
                        type: 'bar',
                        data: {{ labels: dates, datasets: datasets }},
                        options: {{
                            responsive: true,
                            maintainAspectRatio: false,
                            plugins: {{
                                legend: {{ position: 'bottom', labels: {{ font: {{ size: 12 }}, usePointStyle: true }} }},
                                tooltip: {{
                                    callbacks: {{
                                        label: function(ctx) {{ return ctx.dataset.label + ': ' + ctx.parsed.y.toLocaleString(undefined, {{minimumFractionDigits: 2, maximumFractionDigits: 2}}); }}
                                    }}
                                }}
                            }},
                            scales: {{
                                x: {{ stacked: true, ticks: {{ maxRotation: 45, font: {{ size: 11 }} }} }},
                                y: {{ stacked: true, ticks: {{ callback: function(v) {{ return v.toLocaleString(); }} }} }}
                            }},
                            onClick: onBarClick(dates, labelIds)
                        }}
                    }});
                }}
            }};

            window.showExpensesEmpty = function(msg) {{
                document.getElementById('expenses-canvas').style.display = 'none';
                var el = document.getElementById('expenses-empty');
                el.innerHTML = '<p>' + msg + '</p>';
                el.style.display = '';
            }};

            window.toggleLabel = function(btn) {{
                var id = btn.dataset.labelId;
                if (selectedLabels.has(id)) {{
                    selectedLabels.delete(id);
                    btn.classList.remove('label-pill-active');
                }} else {{
                    selectedLabels.add(id);
                    btn.classList.add('label-pill-active');
                }}
                loadChart();
            }};

            window.selectExpensePeriod = function(btn, days) {{
                document.querySelectorAll('#expenses-controls .period-btn')
                    .forEach(function(b) {{ b.classList.remove('period-btn-active'); }});
                btn.classList.add('period-btn-active');
                currentDays = days;
                loadChart();
            }};

            function loadChart() {{
                if (selectedLabels.size === 0) {{
                    document.getElementById('expenses-canvas').style.display = 'none';
                    document.getElementById('expenses-empty').innerHTML =
                        '<p>' + selectLabelsMsg + '</p>';
                    document.getElementById('expenses-empty').style.display = '';
                    document.getElementById('expenses-txn-card').style.display = 'none';
                    if (expensesChart) {{ expensesChart.destroy(); expensesChart = null; }}
                    return;
                }}
                var ids = Array.from(selectedLabels).join(',');
                var url = basePath + '/leanfin/expenses/chart?label_ids=' + ids + '&days=' + currentDays;
                htmx.ajax('GET', url, '#expenses-data');
                loadTransactions(ids, null, null);
            }}

            window.loadTransactions = function(ids, dateFrom, dateTo) {{
                var txnUrl = basePath + '/leanfin/transactions?label_ids=' + (ids || Array.from(selectedLabels).join(','));
                if (dateFrom) txnUrl += '&date_from=' + dateFrom;
                if (dateTo) txnUrl += '&date_to=' + dateTo;
                var card = document.getElementById('expenses-txn-card');
                card.style.display = '';
                var rangeEl = document.getElementById('expenses-txn-range');
                if (dateFrom && dateTo) {{
                    rangeEl.textContent = dateFrom + ' to ' + dateTo;
                }} else {{
                    rangeEl.textContent = '';
                }}
                htmx.ajax('GET', txnUrl, '#expenses-txn-table');
            }};
        }})();
        </script>"##,
        title = t.exp_title,
        subtitle = t.exp_subtitle,
        select_labels = t.exp_select_labels,
        transactions = t.exp_transactions,
        select_labels_js = t.exp_select_labels,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.expenses),
        &leanfin_nav(base, "expenses", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct ChartQuery {
    label_ids: String,
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
            // Sunday = end of ISO week
            let days_until_sunday = (7 - d.weekday().num_days_from_monday()) % 7;
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

async fn chart_data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<ChartQuery>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    let label_ids: Vec<i64> = params
        .label_ids
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();

    if label_ids.is_empty() {
        return Html(format!(
            "<script>window.showExpensesEmpty({:?});</script>",
            t.exp_no_selected
        ));
    }

    let raw = expenses::get_expense_series(&state.pool, user_id.0, &label_ids, params.days)
        .await
        .unwrap_or_default();
    let series = downsample_expenses(&raw, params.days);

    if series.is_empty() {
        return Html(format!(
            "<script>window.showExpensesEmpty({:?});</script>",
            t.exp_no_data
        ));
    }

    // Collect all unique dates and label info
    let all_dates: BTreeSet<&str> = series.iter().map(|p| p.date.as_str()).collect();
    let mut label_info: Vec<(i64, String, String)> = Vec::new(); // (id, name, color)
    let mut seen_labels = std::collections::HashSet::new();
    for p in &series {
        if seen_labels.insert(p.label_id) {
            label_info.push((
                p.label_id,
                p.label_name.clone(),
                p.label_color
                    .clone()
                    .unwrap_or_else(|| "#6B6B6B".to_string()),
            ));
        }
    }

    // Build a map: (date, label_id) -> total
    let mut data_map: HashMap<(&str, i64), f64> = HashMap::new();
    for p in &series {
        data_map.insert((p.date.as_str(), p.label_id), p.total);
    }

    // Build JSON labels (dates)
    let dates_json: Vec<String> = all_dates.iter().map(|d| format!("\"{d}\"")).collect();

    // Build Chart.js datasets: one per label
    let mut datasets_json = Vec::new();
    for (lid, name, color) in &label_info {
        let values: Vec<String> = all_dates
            .iter()
            .map(|d| {
                let v = data_map.get(&(*d, *lid)).copied().unwrap_or(0.0);
                format!("{v:.2}")
            })
            .collect();
        datasets_json.push(format!(
            r#"{{ label: "{name}", data: [{vals}], backgroundColor: '{color}' }}"#,
            vals = values.join(","),
        ));
    }

    let label_ids_str = label_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    Html(format!(
        r#"<script>window.updateExpensesChart([{dates}],[{datasets}],'{label_ids_str}');</script>"#,
        dates = dates_json.join(","),
        datasets = datasets_json.join(","),
    ))
}
