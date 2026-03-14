use axum::{Extension, Router, response::Html, routing::get};
use serde::Deserialize;

use super::dashboard::leanfin_nav;
use super::services::balance;
use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;

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
) -> Html<String> {
    let base = &state.config.base_path;

    let accounts: Vec<AccountOption> = sqlx::query_as(
        "SELECT id, bank_name, iban, account_type, account_name FROM leanfin_accounts WHERE user_id = ? AND archived = 0 ORDER BY bank_name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if accounts.is_empty() {
        let body = r#"<div class="page-header">
            <h1>Balance Evolution</h1>
            <p>Track how your account balances change over time</p>
        </div>
        <div class="card">
            <div class="empty-state"><p>No accounts yet. Link a bank account or add a manual account first.</p></div>
        </div>"#;
        return Html(render_page(
            "LeanFin — Balance",
            &leanfin_nav(base, "balance"),
            body,
            base,
        ));
    }

    let mut account_options = String::from(r#"<option value="">All accounts</option>"#);
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
            <h1>Balance Evolution</h1>
            <p>Track how your account balances change over time</p>
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
            <div id="balance-data"
                 hx-get="{base}/leanfin/balance-evolution/data?account_id=&days=90"
                 hx-trigger="load, sync-done from:body">
                <div class="loading">Loading balance data</div>
            </div>
        </div>
        <div class="card mt-2" id="balance-txn-card" style="display:none">
            <div class="card-header">
                <h2>Transactions</h2>
                <span id="balance-txn-date" class="text-sm text-secondary"></span>
            </div>
            <div id="balance-txn-table"></div>
        </div>
        <script>
        (function() {{
            var basePath = '{base}';

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
    );

    Html(render_page(
        "LeanFin — Balance",
        &leanfin_nav(base, "balance"),
        &body,
        base,
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

async fn data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    axum::extract::Query(params): axum::extract::Query<DataQuery>,
) -> Html<String> {
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
            return Html("<div class=\"empty-state\"><p>Account not found.</p></div>".into());
        }

        balance::get_balance_series(&state.pool, account_id, params.days)
            .await
            .unwrap_or_default()
    } else {
        balance::get_aggregated_balance_series(&state.pool, user_id.0, params.days)
            .await
            .unwrap_or_default()
    };

    if series.is_empty() {
        return Html(
            "<div class=\"empty-state\"><p>No balance data yet. Run a sync to populate balance history.</p></div>"
                .into(),
        );
    }

    // Build JSON arrays for Frappe Charts
    let labels: Vec<String> = series.iter().map(|p| format!("\"{}\"", p.date)).collect();
    let values: Vec<String> = series.iter().map(|p| format!("{:.2}", p.balance)).collect();
    let labels_json = format!("[{}]", labels.join(","));
    let values_json = format!("[{}]", values.join(","));

    let account_id_str = params.account_id.map_or(String::new(), |id| id.to_string());

    let html = format!(
        r##"<div id="balance-chart" class="frappe-chart-container"></div>
        <script>
        (function() {{
            var el = document.getElementById('balance-chart');
            if (!el) return;
            el.innerHTML = '';
            var dates = {labels_json};
            var accountId = '{account_id_str}';
            var chart = new frappe.Chart(el, {{
                data: {{
                    labels: dates,
                    datasets: [{{ values: {values_json} }}]
                }},
                type: 'line',
                height: 300,
                colors: ['#1A6B5A'],
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
            chart.parent.addEventListener('data-select', function(e) {{
                var idx = e.index;
                if (idx != null && dates[idx]) {{
                    window.loadBalanceTxn(accountId, dates[idx]);
                }}
            }});
        }})();
        </script>"##,
    );

    Html(html)
}
