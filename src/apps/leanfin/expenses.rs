use axum::{Extension, Router, response::Html, routing::get};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

use super::dashboard::leanfin_nav;
use super::services::expenses;
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::render_page;
use crate::routes::AppState;

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
    let t = i18n::t(lang);

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
            title = t.lf_exp_title,
            subtitle = t.lf_exp_subtitle,
            no_labels = t.lf_exp_no_labels,
        );
        return Html(render_page(
            &format!("LeanFin — {}", t.lf_expenses),
            &leanfin_nav(base, "expenses", lang),
            &body,
            base,
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
            <div id="expenses-chart">
                <div class="empty-state"><p>{select_labels}</p></div>
            </div>
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
            var currentChart = null;
            var selectLabelsMsg = '{select_labels_js}';

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
                    document.getElementById('expenses-chart').innerHTML =
                        '<div class="empty-state"><p>' + selectLabelsMsg + '</p></div>';
                    document.getElementById('expenses-txn-card').style.display = 'none';
                    return;
                }}
                var ids = Array.from(selectedLabels).join(',');
                var url = basePath + '/leanfin/expenses/chart?label_ids=' + ids + '&days=' + currentDays;
                fetch(url).then(function(r) {{ return r.text(); }}).then(function(html) {{
                    document.getElementById('expenses-chart').innerHTML = html;
                    // Execute scripts in the response
                    var scripts = document.getElementById('expenses-chart').querySelectorAll('script');
                    scripts.forEach(function(s) {{
                        var ns = document.createElement('script');
                        ns.textContent = s.textContent;
                        s.parentNode.replaceChild(ns, s);
                    }});
                }});
                // Load transactions filtered by the selected labels
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
        title = t.lf_exp_title,
        subtitle = t.lf_exp_subtitle,
        select_labels = t.lf_exp_select_labels,
        transactions = t.lf_exp_transactions,
        select_labels_js = t.lf_exp_select_labels,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.lf_expenses),
        &leanfin_nav(base, "expenses", lang),
        &body,
        base,
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

async fn chart_data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<ChartQuery>,
) -> Html<String> {
    let t = i18n::t(lang);

    let label_ids: Vec<i64> = params
        .label_ids
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();

    if label_ids.is_empty() {
        return Html(format!(
            "<div class=\"empty-state\"><p>{}</p></div>",
            t.lf_exp_no_selected
        ));
    }

    let series = expenses::get_expense_series(&state.pool, user_id.0, &label_ids, params.days)
        .await
        .unwrap_or_default();

    if series.is_empty() {
        return Html(format!(
            "<div class=\"empty-state\"><p>{}</p></div>",
            t.lf_exp_no_data
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

    // Build datasets: one per label
    let mut datasets_json = Vec::new();
    for (lid, name, _) in &label_info {
        let values: Vec<String> = all_dates
            .iter()
            .map(|d| {
                let v = data_map.get(&(*d, *lid)).copied().unwrap_or(0.0);
                format!("{v:.2}")
            })
            .collect();
        datasets_json.push(format!(
            r#"{{ name: "{name}", values: [{vals}] }}"#,
            vals = values.join(","),
        ));
    }

    // Add a "Total" dataset when multiple labels are selected
    if label_info.len() > 1 {
        let values: Vec<String> = all_dates
            .iter()
            .map(|d| {
                let total: f64 = label_info
                    .iter()
                    .map(|(lid, _, _)| data_map.get(&(*d, *lid)).copied().unwrap_or(0.0))
                    .sum();
                format!("{total:.2}")
            })
            .collect();
        datasets_json.push(format!(
            r#"{{ name: "Total", values: [{vals}] }}"#,
            vals = values.join(","),
        ));
    }

    let mut colors: Vec<String> = label_info
        .iter()
        .map(|(_, _, c)| format!("'{c}'"))
        .collect();
    if label_info.len() > 1 {
        colors.push("'#000000'".to_string());
    }

    let label_ids_str = label_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let html = format!(
        r##"<div id="expenses-frappe-chart" class="frappe-chart-container"></div>
        <script>
        (function() {{
            var el = document.getElementById('expenses-frappe-chart');
            if (!el) return;
            el.innerHTML = '';
            var chart = new frappe.Chart(el, {{
                data: {{
                    labels: [{dates}],
                    datasets: [{datasets}]
                }},
                type: 'line',
                height: 300,
                colors: [{colors}],
                lineOptions: {{
                    regionFill: 1,
                    hideDots: 0
                }},
                axisOptions: {{
                    xIsSeries: true,
                    xAxisMode: 'tick'
                }},
                tooltipOptions: {{
                    formatTooltipY: function(d) {{ return d.toLocaleString(undefined, {{minimumFractionDigits: 2, maximumFractionDigits: 2}}); }}
                }},
                isNavigable: true
            }});
            var dates = [{dates}];
            chart.parent.addEventListener('data-select', function(e) {{
                var idx = e.index;
                if (idx != null && dates[idx]) {{
                    var d = dates[idx];
                    window.loadTransactions('{label_ids_str}', d, d);
                }}
            }});
        }})();
        </script>"##,
        dates = dates_json.join(","),
        datasets = datasets_json.join(","),
        colors = colors.join(","),
    );

    Html(html)
}
