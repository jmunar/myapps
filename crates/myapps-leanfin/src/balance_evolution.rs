//! Balance tab: how one account's balance — or every account's together —
//! moved over a window of whole calendar periods.
//!
//! The window comes from `period`, so the points line up bucket for bucket with
//! the Breakdown tab: one point per day, week or month, ending at the last
//! complete one, plus the running period when the selector's `+` is on.

use axum::{Extension, Router, response::Html, routing::get};
use chrono::NaiveDate;
use serde::Deserialize;

use super::dashboard::leanfin_nav;
use super::period::{self, Window};
use super::services::balance::{self, BalancePoint};
use myapps_core::auth::UserId;
use myapps_core::components::html_escape;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

/// Chart wiring for this page. Kept in its own file so the Rust side never has
/// to brace-escape a page of JavaScript.
const BALANCE_JS: &str = include_str!("../static/balance-evolution.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/balance-evolution", get(page))
        .route("/balance-evolution/data", get(data))
}

#[derive(sqlx::FromRow)]
struct AccountOption {
    id: i64,
    bank_name: String,
    account_name: Option<String>,
}

#[derive(Deserialize, Default)]
struct PageParams {
    /// Lenient on purpose: this one arrives in a URL a person can edit, so a
    /// junk value falls back to the aggregate view rather than a 400.
    #[serde(default)]
    account_id: Option<String>,
}

async fn page(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<PageParams>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let accounts: Vec<AccountOption> = sqlx::query_as(
        "SELECT id, bank_name, account_name FROM leanfin_accounts WHERE user_id = ? AND archived = 0 ORDER BY bank_name",
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

    // A deep link from the Accounts tab preselects its account; an id that is
    // not the user's own (or is archived, so it has no option) falls back to
    // the aggregate view.
    let selected_id = params
        .account_id
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|id| accounts.iter().any(|a| a.id == *id));

    let mut account_options = format!(
        r#"<option value=""{}>{}</option>"#,
        if selected_id.is_none() {
            " selected"
        } else {
            ""
        },
        t.txn_all_accounts,
    );
    for a in &accounts {
        // Whatever the account is called, and nothing else: an IBAN here is
        // 24 characters of noise on a phone, and two accounts at the same bank
        // are told apart by renaming them.
        account_options.push_str(&format!(
            r#"<option value="{}"{}>{}</option>"#,
            a.id,
            if selected_id == Some(a.id) {
                " selected"
            } else {
                ""
            },
            html_escape(a.account_name.as_deref().unwrap_or(&a.bank_name)),
        ));
    }

    let initial_account = selected_id.map_or(String::new(), |id| id.to_string());
    let window = period::DEFAULT_WINDOW.key();
    let current = if period::DEFAULT_INCLUDE_CURRENT {
        "1"
    } else {
        "0"
    };
    let selector = period::render_selector(
        period::DEFAULT_WINDOW,
        period::DEFAULT_INCLUDE_CURRENT,
        "balanceWindowChanged",
        lang,
    );

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="balance-controls" id="balance-controls" data-base="{base}">
                <select name="account_id"
                        class="txn-filter-select"
                        hx-get="{base}/leanfin/balance-evolution/data"
                        hx-target="#balance-data"
                        hx-trigger="change"
                        hx-include="#balance-controls">
                    {account_options}
                </select>
                {selector}
                <input type="hidden" name="window" id="balance-window" value="{window}">
                <input type="hidden" name="current" id="balance-current" value="{current}">
            </div>
            <div class="chart-container"><canvas id="balance-canvas"></canvas></div>
            <div id="balance-empty" class="empty-state" style="display:none"></div>
            <div id="balance-data"
                 hx-get="{base}/leanfin/balance-evolution/data?account_id={initial_account}&window={window}&current={current}"
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
        <script>{selector_js}</script>
        <script>{balance_js}</script>"##,
        title = t.bal_title,
        subtitle = t.bal_subtitle,
        transactions = t.exp_transactions,
        selector_js = period::SELECTOR_JS,
        balance_js = BALANCE_JS,
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
    #[serde(default)]
    window: Option<String>,
    #[serde(default)]
    current: Option<String>,
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

/// Pick the balance each bucket closed on: the last daily value at or before
/// its end date. Buckets that end before the account has any history at all
/// are dropped rather than plotted at zero, which would read as an empty
/// account rather than an unknown one.
fn downsample_balance(series: &[BalancePoint], ends: &[NaiveDate]) -> Vec<(usize, f64)> {
    let mut points: Vec<(NaiveDate, f64)> = series
        .iter()
        .filter_map(|p| {
            NaiveDate::parse_from_str(&p.date, "%Y-%m-%d")
                .ok()
                .map(|d| (d, p.balance))
        })
        .collect();
    points.sort_by_key(|(d, _)| *d);

    let mut out = Vec::new();
    let mut cursor = 0;
    let mut last: Option<f64> = None;
    for (i, end) in ends.iter().enumerate() {
        while cursor < points.len() && points[cursor].0 <= *end {
            last = Some(points[cursor].1);
            cursor += 1;
        }
        if let Some(balance) = last {
            out.push((i, balance));
        }
    }
    out
}

async fn data(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    axum::extract::Query(params): axum::extract::Query<DataQuery>,
) -> Html<String> {
    let t = super::i18n::t(lang);

    let window = Window::parse(params.window.as_deref());
    let include_current = match params.current.as_deref() {
        Some(v) => v == "1" || v == "true",
        None => period::DEFAULT_INCLUDE_CURRENT,
    };
    let today = chrono::Utc::now().date_naive();
    let periods = period::periods(window, include_current, today);
    // The series query counts back from today, so the window start becomes a
    // number of days. Buckets that reach past today simply take today's value.
    let days = (today - periods.start()).num_days().max(0);

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
            return Html(empty_script(t.bal_account_not_found));
        }

        balance::get_balance_series(&state.pool, account_id, days)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            })
    } else {
        balance::get_aggregated_balance_series(&state.pool, user_id.0, days)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            })
    };

    let ends: Vec<NaiveDate> = periods.periods.iter().map(|p| p.end).collect();
    let plotted = downsample_balance(&series, &ends);

    if plotted.is_empty() {
        return Html(empty_script(t.bal_no_data));
    }

    let payload = serde_json::json!({
        "accountId": params.account_id.map_or(String::new(), |id| id.to_string()),
        "dates": plotted
            .iter()
            .map(|(i, _)| period::iso(periods.periods[*i].end))
            .collect::<Vec<_>>(),
        "starts": plotted
            .iter()
            .map(|(i, _)| period::iso(periods.periods[*i].start))
            .collect::<Vec<_>>(),
        "values": plotted
            .iter()
            .map(|(_, b)| (b * 100.0).round() / 100.0)
            .collect::<Vec<_>>(),
    });

    Html(format!(
        "<script>window.updateBalanceChart({});</script>",
        json_for_script(&payload)
    ))
}

fn empty_script(message: &str) -> String {
    format!(
        "<script>window.showBalanceEmpty({});</script>",
        json_for_script(&serde_json::Value::from(message))
    )
}

/// Serialize for embedding in a `<script>` body — see the note on the twin in
/// `breakdown.rs`.
fn json_for_script(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}
