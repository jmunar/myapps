use axum::{Extension, Router, response::Html, routing::get};

use crate::auth::UserId;
use crate::layout::{NavItem, render_page};
use crate::routes::AppState;
use super::sync_handler::sync_button;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn leanfin_nav(base: &str, active: &str) -> Vec<NavItem> {
    vec![
        NavItem { href: format!("{base}/leanfin"), label: "LeanFin", active: false },
        NavItem { href: format!("{base}/leanfin"), label: "Transactions", active: active == "transactions" },
        NavItem { href: format!("{base}/leanfin/accounts"), label: "Accounts", active: active == "accounts" },
        NavItem { href: format!("{base}/leanfin/balance-evolution"), label: "Balance", active: active == "balance" },
        NavItem { href: format!("{base}/leanfin/labels"), label: "Labels", active: active == "labels" },
        NavItem { href: format!("{base}/logout"), label: "Log out", active: false },
    ]
}

#[derive(sqlx::FromRow)]
struct AccountOption {
    id: i64,
    bank_name: String,
    iban: Option<String>,
    account_type: String,
    account_name: Option<String>,
}

async fn index(
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

    let sync_btn = sync_button(base);
    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>Transactions</h1>
                <div class="sync-container" id="sync-container">
                    {sync_btn}
                </div>
            </div>
            <p>Your recent activity across all accounts</p>
        </div>
        <div class="card">
            <div class="txn-filters" id="txn-filters">
                <input type="search" name="q" placeholder="Search transactions…"
                       class="txn-filter-search"
                       hx-get="{base}/leanfin/transactions"
                       hx-target="#txn-table"
                       hx-trigger="input changed delay:200ms"
                       hx-include="#txn-filters">
                <select name="account_id"
                        class="txn-filter-select"
                        hx-get="{base}/leanfin/transactions"
                        hx-target="#txn-table"
                        hx-trigger="change"
                        hx-include="#txn-filters">
                    {account_options}
                </select>
                <label class="txn-filter-check">
                    <input type="checkbox" name="unallocated" value="1"
                           hx-get="{base}/leanfin/transactions"
                           hx-target="#txn-table"
                           hx-trigger="change"
                           hx-include="#txn-filters">
                    Not fully allocated
                </label>
            </div>
            <div id="txn-table" hx-get="{base}/leanfin/transactions" hx-trigger="load, sync-done from:body">
                <div class="loading">Loading transactions</div>
            </div>
        </div>"##
    );
    Html(render_page("LeanFin — Transactions", &leanfin_nav(base, "transactions"), &body, base))
}
