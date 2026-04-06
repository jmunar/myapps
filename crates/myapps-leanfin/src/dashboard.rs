use axum::{Extension, Router, response::Html, routing::get};

use super::sync_handler::sync_button;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::{NavItem, render_page};
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn leanfin_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = super::i18n::t(lang);
    let ct = myapps_core::i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/leanfin"),
            label: "LeanFin".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin"),
            label: t.transactions.to_string(),
            active: active == "transactions",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/accounts"),
            label: t.accounts.to_string(),
            active: active == "accounts",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/balance-evolution"),
            label: t.balance.to_string(),
            active: active == "balance",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/expenses"),
            label: t.expenses.to_string(),
            active: active == "expenses",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/labels"),
            label: t.labels.to_string(),
            active: active == "labels",
            right: false,
        },
        NavItem {
            href: format!("{base}/leanfin/settings"),
            label: t.settings.to_string(),
            active: active == "settings",
            right: false,
        },
        NavItem {
            href: format!("{base}/logout"),
            label: ct.log_out.to_string(),
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
    let t = super::i18n::t(lang);

    let accounts: Vec<AccountOption> = sqlx::query_as(
        "SELECT id, bank_name, iban, account_type, account_name FROM leanfin_accounts WHERE user_id = ? ORDER BY bank_name",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

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

    let labels: Vec<LabelOption> =
        sqlx::query_as("SELECT id, name FROM leanfin_labels WHERE user_id = ? ORDER BY name")
            .bind(user_id.0)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("DB query failed: {e:#}");
                Default::default()
            });

    let mut label_options = format!(r#"<option value="">{}</option>"#, t.txn_all_labels);
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
        title = t.txn_title,
        subtitle = t.txn_subtitle,
        search = t.txn_search,
        not_allocated = t.txn_not_allocated,
        loading = t.txn_loading,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.transactions),
        &leanfin_nav(base, "transactions", lang),
        &body,
        &state.config,
        lang,
    ))
}
