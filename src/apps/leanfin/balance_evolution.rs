use axum::{Extension, Router, response::Html, routing::get};
use serde::Deserialize;

use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;
use super::dashboard::leanfin_nav;
use super::services::balance;

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
        "SELECT id, bank_name, iban, account_type, account_name FROM accounts WHERE user_id = ? ORDER BY bank_name",
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
        return Html(render_page("LeanFin — Balance", &leanfin_nav(base, "balance"), body, base));
    }

    let mut account_options = String::from(r#"<option value="">All accounts</option>"#);
    for a in &accounts {
        let display = if a.account_type == "manual" {
            a.account_name.clone().unwrap_or_else(|| a.bank_name.clone())
        } else {
            match &a.iban {
                Some(iban) => format!("{} ({})", a.bank_name, iban),
                None => a.bank_name.clone(),
            }
        };
        account_options.push_str(&format!(
            r#"<option value="{}">{}</option>"#,
            a.id, display,
        ));
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
        <script>
        function selectPeriod(btn, days) {{
            document.querySelectorAll('.period-btn').forEach(b => b.classList.remove('period-btn-active'));
            btn.classList.add('period-btn-active');
            document.getElementById('balance-days').value = days;
            htmx.trigger(document.querySelector('#balance-controls select'), 'change');
        }}
        </script>"##,
    );

    Html(render_page("LeanFin — Balance", &leanfin_nav(base, "balance"), &body, base))
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
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ? AND user_id = ?)",
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

    // Build SVG chart
    let chart_html = render_chart(&series);

    // Build data table
    let mut rows = String::new();
    for point in series.iter().rev() {
        let sign = if point.balance < 0.0 { "negative" } else { "positive" };
        let source_badge = match point.source.as_str() {
            "reported" => r#"<span class="source-badge source-reported">reported</span>"#,
            "aggregated" => r#"<span class="source-badge source-aggregated">aggregated</span>"#,
            "carried" => r#"<span class="source-badge source-computed">carried</span>"#,
            _ => r#"<span class="source-badge source-computed">computed</span>"#,
        };
        rows.push_str(&format!(
            r#"<tr>
                <td class="txn-date">{}</td>
                <td class="txn-amount {sign}">{:.2}</td>
                <td>{source_badge}</td>
            </tr>"#,
            point.date, point.balance,
        ));
    }

    let html = format!(
        r#"{chart_html}
        <table>
            <thead>
                <tr>
                    <th>Date</th>
                    <th>Balance</th>
                    <th>Source</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>"#,
    );

    Html(html)
}

fn render_chart(series: &[balance::BalancePoint]) -> String {
    if series.len() < 2 {
        return String::new();
    }

    let width = 800.0_f64;
    let height = 200.0_f64;
    let pad_x = 60.0_f64;
    let pad_y = 20.0_f64;

    let min_bal = series.iter().map(|p| p.balance).fold(f64::INFINITY, f64::min);
    let max_bal = series.iter().map(|p| p.balance).fold(f64::NEG_INFINITY, f64::max);
    let range = if (max_bal - min_bal).abs() < 0.01 { 1.0 } else { max_bal - min_bal };

    let n = series.len() as f64;
    let points: Vec<String> = series
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = pad_x + (i as f64 / (n - 1.0)) * (width - 2.0 * pad_x);
            let y = pad_y + (1.0 - (p.balance - min_bal) / range) * (height - 2.0 * pad_y);
            format!("{x:.1},{y:.1}")
        })
        .collect();

    let polyline = points.join(" ");

    // Fill area
    let first_x = pad_x;
    let last_x = pad_x + (width - 2.0 * pad_x);
    let bottom = height - pad_y;
    let fill_points = format!("{first_x:.1},{bottom:.1} {polyline} {last_x:.1},{bottom:.1}");

    // Y-axis labels
    let fmt = |v: f64| -> String {
        if v.abs() >= 1000.0 {
            format!("{:.0}k", v / 1000.0)
        } else {
            format!("{v:.0}")
        }
    };
    let mid = min_bal + range / 2.0;

    // X-axis labels (first, middle, last)
    let first_date = &series[0].date;
    let last_date = &series[series.len() - 1].date;
    let mid_idx = series.len() / 2;
    let mid_date = &series[mid_idx].date;

    format!(
        r#"<div class="balance-chart">
            <svg viewBox="0 0 {width} {height}" preserveAspectRatio="none" class="balance-svg">
                <polygon points="{fill_points}" class="chart-fill"/>
                <polyline points="{polyline}" class="chart-line"/>
            </svg>
            <div class="chart-y-labels">
                <span>{}</span>
                <span>{}</span>
                <span>{}</span>
            </div>
            <div class="chart-x-labels">
                <span>{first_date}</span>
                <span>{mid_date}</span>
                <span>{last_date}</span>
            </div>
        </div>"#,
        fmt(max_bal), fmt(mid), fmt(min_bal),
    )
}
