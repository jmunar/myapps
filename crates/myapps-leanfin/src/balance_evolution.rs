use axum::{Extension, Router, response::Html, routing::get};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;

use super::dashboard::leanfin_nav;
use super::services::balance::{self, BalancePoint};
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/balance-evolution", get(page))
        .route("/balance-evolution/data", get(data))
}

#[derive(sqlx::FromRow)]
struct AccountOption {
    id: i64,
    bank_name: String,
    iban: Option<String>,
    account_type: String,
    account_name: Option<String>,
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let accounts: Vec<AccountOption> = sqlx::query_as(
        "SELECT id, bank_name, iban, account_type, account_name FROM leanfin_accounts WHERE user_id = ? AND archived = 0 ORDER BY bank_name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    if accounts.is_empty() {
        let body = format!(
            r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="empty-state"><p>{no_accounts}</p></div>
        </div>"#,
            title = t.bal_title,
            subtitle = t.bal_subtitle,
            no_accounts = t.bal_no_accounts,
        );
        return Html(render_page(
            &format!("LeanFin — {}", t.balance),
            &leanfin_nav(base, "balance", lang),
            &body,
            &state.config,
            lang,
        ));
    }

    let mut account_options = format!(r#"<option value="">{}</option>"#, t.txn_all_accounts);
    for a in &accounts {
        let display = if a.account_type == "manual" {
            a.account_name
                .clone()
                .unwrap_or_else(|| a.bank_name.clone())
        } else {
            match &a.iban {
                Some(iban) => format!("{} ({})", a.bank_name, iban),
                None => a.bank_name.clone(),
            }
        };
        account_options.push_str(&format!(r#"<option value="{}">{}</option>"#, a.id, display,));
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="balance-controls" id="balance-controls">
                <select name="account_id"
                        class="txn-filter-select"
                        hx-get="{base}/leanfin/balance-evolution/data"
                        hx-target="#balance-data"
                        hx-trigger="change"
                        hx-include="#balance-controls">
                    {account_options}
                </select>
                <div class="period-selector">
                    <button type="button" class="period-btn" data-days="30"
                            onclick="selectPeriod(this, 30)">30d</button>
                    <button type="button" class="period-btn period-btn-active" data-days="90"
                            onclick="selectPeriod(this, 90)">90d</button>
                    <button type="button" class="period-btn" data-days="180"
                            onclick="selectPeriod(this, 180)">180d</button>
                    <button type="button" class="period-btn" data-days="365"
                            onclick="selectPeriod(this, 365)">365d</button>
                </div>
                <input type="hidden" name="days" id="balance-days" value="90">
            </div>
            <div class="chart-container"><canvas id="balance-canvas"></canvas></div>
            <div id="balance-empty" class="empty-state" style="display:none"></div>
            <div id="balance-data"
                 hx-get="{base}/leanfin/balance-evolution/data?account_id=&days=90"
                 hx-trigger="load, sync-done from:body">
            </div>
        </div>
        <div class="card mt-2" id="balance-txn-card" style="display:none">
            <div class="card-header">
                <h2>{transactions}</h2>
                <span id="balance-txn-date" class="text-sm text-secondary"></span>
            </div>
            <div id="balance-txn-table"></div>
        </div>
        <script>
        (function() {{
            var basePath = '{base}';
            var balanceChart = null;

            window.updateBalanceChart = function(dates, values, accountId) {{
                var canvas = document.getElementById('balance-canvas');
                var emptyEl = document.getElementById('balance-empty');
                if (dates.length === 0) {{
                    canvas.parentElement.style.display = 'none';
                    emptyEl.style.display = '';
                    return;
                }}
                canvas.parentElement.style.display = '';
                emptyEl.style.display = 'none';
                if (balanceChart) {{
                    balanceChart.data.labels = dates;
                    balanceChart.data.datasets[0].data = values;
                    balanceChart.options.onClick = function(evt, elems) {{
                        if (elems.length > 0) window.loadBalanceTxn(accountId, dates[elems[0].index]);
                    }};
                    balanceChart.update();
                }} else {{
                    balanceChart = new Chart(canvas, {{
                        type: 'line',
                        data: {{
                            labels: dates,
                            datasets: [{{
                                data: values,
                                borderColor: '#1A6B5A',
                                backgroundColor: 'rgba(26,107,90,0.15)',
                                fill: true,
                                tension: 0.3,
                                pointRadius: 3,
                                pointHoverRadius: 5
                            }}]
                        }},
                        options: {{
                            responsive: true,
                            maintainAspectRatio: false,
                            plugins: {{
                                legend: {{ display: false }},
                                tooltip: {{
                                    callbacks: {{
                                        label: function(ctx) {{ return ctx.parsed.y.toLocaleString(undefined, {{minimumFractionDigits: 2, maximumFractionDigits: 2}}); }}
                                    }}
                                }}
                            }},
                            scales: {{
                                x: {{ ticks: {{ maxRotation: 45, font: {{ size: 11 }} }} }},
                                y: {{ ticks: {{ callback: function(v) {{ return v.toLocaleString(); }} }} }}
                            }},
                            onClick: function(evt, elems) {{
                                if (elems.length > 0) window.loadBalanceTxn(accountId, dates[elems[0].index]);
                            }}
                        }}
                    }});
                }}
            }};

            window.showBalanceEmpty = function(msg) {{
                document.getElementById('balance-canvas').parentElement.style.display = 'none';
                var el = document.getElementById('balance-empty');
                el.innerHTML = '<p>' + msg + '</p>';
                el.style.display = '';
            }};

            window.selectPeriod = function(btn, days) {{
                document.querySelectorAll('.period-btn').forEach(function(b) {{ b.classList.remove('period-btn-active'); }});
                btn.classList.add('period-btn-active');
                document.getElementById('balance-days').value = days;
                htmx.trigger(document.querySelector('#balance-controls select'), 'change');
                document.getElementById('balance-txn-card').style.display = 'none';
            }};

            window.loadBalanceTxn = function(accountId, date) {{
                var url = basePath + '/leanfin/transactions?date_from=' + date + '&date_to=' + date;
                if (accountId) url += '&account_id=' + accountId;
                var card = document.getElementById('balance-txn-card');
                card.style.display = '';
                document.getElementById('balance-txn-date').textContent = date;
                htmx.ajax('GET', url, '#balance-txn-table');
            }};
        }})();
        </script>"##,
        title = t.bal_title,
        subtitle = t.bal_subtitle,
        transactions = t.exp_transactions,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.balance),
        &leanfin_nav(base, "balance", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct DataQuery {
    #[serde(default, deserialize_with = "deserialize_optional_id")]
    account_id: Option<i64>,
    #[serde(default = "default_days")]
    days: i64,
}

fn default_days() -> i64 {
    90
}

/// Deserialize empty string as None, numeric string as Some(i64).
fn deserialize_optional_id<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        s.parse::<i64>().map(Some).map_err(serde::de::Error::custom)
    }
}

/// Downsample a daily balance series to the appropriate interval:
/// - 30d → daily (no change)
/// - 90d → weekly (Mon–Sun, last value in each week)
/// - 180d/365d → monthly (last value in each calendar month)
fn downsample_balance(series: &[BalancePoint], days: i64) -> Vec<BalancePoint> {
    if days <= 30 || series.is_empty() {
        return series.to_vec();
    }

    let mut result: Vec<BalancePoint> = Vec::new();

    if days <= 90 {
        // Weekly: group by ISO week (Mon–Sun), take last value per week
        let mut current_key: Option<(i32, u32)> = None; // (iso_year, iso_week)
        for p in series {
            if let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") {
                let key = (d.iso_week().year(), d.iso_week().week());
                if current_key == Some(key) {
                    // Same week — replace with latest
                    if let Some(last) = result.last_mut() {
                        *last = p.clone();
                    }
                } else {
                    current_key = Some(key);
                    result.push(p.clone());
                }
            }
        }
    } else {
        // Monthly: group by (year, month), take last value per month
        let mut current_key: Option<(i32, u32)> = None;
        for p in series {
            if let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") {
                let key = (d.year(), d.month());
                if current_key == Some(key) {
                    if let Some(last) = result.last_mut() {
                        *last = p.clone();
                    }
                } else {
                    current_key = Some(key);
                    result.push(p.clone());
                }
            }
        }
    }

    result
}

async fn data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<DataQuery>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    let series = if let Some(account_id) = params.account_id {
        // Verify account belongs to user
        let owns: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM leanfin_accounts WHERE id = ? AND user_id = ?)",
        )
        .bind(account_id)
        .bind(user_id.0)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

        if !owns {
            return Html(format!(
                "<script>window.showBalanceEmpty({:?});</script>",
                t.bal_account_not_found
            ));
        }

        balance::get_balance_series(&state.pool, account_id, params.days)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            })
    } else {
        balance::get_aggregated_balance_series(&state.pool, user_id.0, params.days)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            })
    };

    let series = downsample_balance(&series, params.days);

    if series.is_empty() {
        return Html(format!(
            "<script>window.showBalanceEmpty({:?});</script>",
            t.bal_no_data
        ));
    }

    // Build JSON arrays for Chart.js
    let labels: Vec<String> = series.iter().map(|p| format!("\"{}\"", p.date)).collect();
    let values: Vec<String> = series.iter().map(|p| format!("{:.2}", p.balance)).collect();
    let labels_json = format!("[{}]", labels.join(","));
    let values_json = format!("[{}]", values.join(","));

    let account_id_str = params.account_id.map_or(String::new(), |id| id.to_string());

    Html(format!(
        r#"<script>window.updateBalanceChart({labels_json},{values_json},'{account_id_str}');</script>"#,
    ))
}
