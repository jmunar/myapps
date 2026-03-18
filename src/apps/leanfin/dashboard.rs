use axum::{Extension, Router, response::Html, routing::get};

use super::sync_handler::sync_button;
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::{NavItem, render_page};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn leanfin_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/leanfin"),
            label: "LeanFin".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin"),
            label: t.lf_transactions.to_string(),
            active: active == "transactions",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/accounts"),
            label: t.lf_accounts.to_string(),
            active: active == "accounts",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/balance-evolution"),
            label: t.lf_balance.to_string(),
            active: active == "balance",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/expenses"),
            label: t.lf_expenses.to_string(),
            active: active == "expenses",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/labels"),
            label: t.lf_labels.to_string(),
            active: active == "labels",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/settings"),
            label: t.lf_settings.to_string(),
            active: active == "settings",
            right: false,
        },
        NavItem {
            href: format!("{base}/logout"),
            label: t.log_out.to_string(),
            active: false,
            right: true,
        },
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

#[derive(sqlx::FromRow)]
struct LabelOption {
    id: i64,
    name: String,
}

async fn index(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let accounts: Vec<AccountOption> = sqlx::query_as(
        "SELECT id, bank_name, iban, account_type, account_name FROM leanfin_accounts WHERE user_id = ? ORDER BY bank_name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut account_options = format!(r#"<option value="">{}</option>"#, t.lf_txn_all_accounts);
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

    let labels: Vec<LabelOption> =
        sqlx::query_as("SELECT id, name FROM leanfin_labels WHERE user_id = ? ORDER BY name")
            .bind(user_id.0)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    let mut label_options = format!(r#"<option value="">{}</option>"#, t.lf_txn_all_labels);
    for l in &labels {
        label_options.push_str(&format!(r#"<option value="{}">{}</option>"#, l.id, l.name,));
    }

    let sync_btn = sync_button(base, lang);
    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>{title}</h1>
                <div class="sync-container" id="sync-container">
                    {sync_btn}
                </div>
            </div>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="txn-filters" id="txn-filters">
                <input type="search" name="q" placeholder="{search}"
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
                <select name="label_ids"
                        class="txn-filter-select"
                        hx-get="{base}/leanfin/transactions"
                        hx-target="#txn-table"
                        hx-trigger="change"
                        hx-include="#txn-filters">
                    {label_options}
                </select>
                <label class="txn-filter-check">
                    <input type="checkbox" name="unallocated" value="1"
                           hx-get="{base}/leanfin/transactions"
                           hx-target="#txn-table"
                           hx-trigger="change"
                           hx-include="#txn-filters">
                    {not_allocated}
                </label>
            </div>
            <div id="txn-table" hx-get="{base}/leanfin/transactions" hx-trigger="load, sync-done from:body">
                <div class="loading">{loading}</div>
            </div>
        </div>"##,
        title = t.lf_txn_title,
        subtitle = t.lf_txn_subtitle,
        search = t.lf_txn_search,
        not_allocated = t.lf_txn_not_allocated,
        loading = t.lf_txn_loading,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.lf_transactions),
        &leanfin_nav(base, "transactions", lang),
        &body,
        base,
        lang,
    ))
}
