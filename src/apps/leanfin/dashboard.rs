use axum::{Router, response::Html, routing::get};

use crate::layout::{NavItem, render_page};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn leanfin_nav(base: &str, active: &str) -> Vec<NavItem> {
    vec![
        NavItem { href: format!("{base}/leanfin"), label: "LeanFin", active: false },
        NavItem { href: format!("{base}/leanfin"), label: "Transactions", active: active == "transactions" },
        NavItem { href: format!("{base}/leanfin/accounts"), label: "Accounts", active: active == "accounts" },
        NavItem { href: format!("{base}/leanfin/labels"), label: "Labels", active: active == "labels" },
        NavItem { href: format!("{base}/logout"), label: "Log out", active: false },
    ]
}

async fn index(
    state: axum::extract::State<AppState>,
) -> Html<String> {
    let base = &state.config.base_path;
    let body = format!(
        r#"<div class="page-header">
            <h1>Transactions</h1>
            <p>Your recent activity across all accounts</p>
        </div>
        <div class="card">
            <div id="transactions" hx-get="{base}/leanfin/transactions" hx-trigger="load">
                <div class="loading">Loading transactions</div>
            </div>
        </div>"#
    );
    Html(render_page("LeanFin — Transactions", &leanfin_nav(base, "transactions"), &body, base))
}
