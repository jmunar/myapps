use axum::{
    Extension, Form, Router,
    extract::{Path, Query},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::NaiveDateTime;
use serde::Deserialize;

use super::dashboard::leanfin_nav;
use super::services::enable_banking;
use super::settings;
use super::sync_handler::sync_button;
use crate::auth::UserId;
use crate::i18n::Lang;
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list_accounts))
        .route("/accounts/link", get(link_form).post(link_submit))
        .route("/accounts/callback", get(callback))
        .route("/accounts/{id}/reauth", post(reauth))
        .route("/accounts/{id}/delete", post(delete_account))
        .route("/accounts/{id}/archive", post(archive_account))
        .route("/accounts/{id}/unarchive", post(unarchive_account))
        .route(
            "/accounts/manual/new",
            get(manual_new_form).post(manual_new_submit),
        )
        .route(
            "/accounts/manual/{id}/edit",
            get(manual_edit_form).post(manual_edit_submit),
        )
        .route(
            "/accounts/manual/{id}/value",
            get(manual_value_form).post(manual_value_submit),
        )
        .route(
            "/accounts/manual/{id}/import-csv",
            get(import_csv_form).post(import_csv_submit),
        )
}

// ── List accounts ─────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct ListAccountsParams {
    show_archived: Option<String>,
    archive_error: Option<i64>,
}

async fn list_accounts(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Query(params): Query<ListAccountsParams>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);
    let show_archived = params.show_archived.is_some();

    let error_banner = if params.archive_error.is_some() {
        format!(
            r#"<div class="alert alert-error">{}</div>"#,
            t.acc_archive_error
        )
    } else {
        String::new()
    };

    let accounts: Vec<AccountRow> = sqlx::query_as(
        "SELECT id, bank_name, iban, session_expires_at, balance_amount, balance_currency, account_type, account_name, asset_category, archived FROM leanfin_accounts WHERE user_id = ?",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let today = chrono::Utc::now().naive_utc();
    let warn_threshold = today + chrono::Duration::days(14);

    let has_archived = accounts.iter().any(|a| a.archived);
    let bank_accounts: Vec<&AccountRow> = accounts
        .iter()
        .filter(|a| a.account_type == "bank" && (show_archived || !a.archived))
        .collect();
    let manual_accounts: Vec<&AccountRow> = accounts
        .iter()
        .filter(|a| a.account_type == "manual" && (show_archived || !a.archived))
        .collect();

    // Bank accounts section
    let mut bank_items = String::new();
    for a in &bank_accounts {
        let session_expires_at = a.session_expires_at;
        let expires = session_expires_at.format("%Y-%m-%d").to_string();
        let iban = a.iban.as_deref().unwrap_or("\u{2014}");
        let balance_html = format_balance(a.balance_amount, a.balance_currency.as_deref());

        if a.archived {
            bank_items.push_str(&format!(
                r#"<div class="account-item account-archived">
                    <div>
                        <div class="account-bank">{bank} <span class="archived-badge">{archived}</span></div>
                        <div class="account-iban">{iban}</div>
                        {balance_html}
                    </div>
                    <div class="account-actions">
                        <form method="POST" action="{base}/leanfin/accounts/{id}/unarchive" style="display:inline">
                            <button type="submit" class="btn-icon">{unarchive}</button>
                        </form>
                        <form method="POST" action="{base}/leanfin/accounts/{id}/delete"
                              onsubmit="return confirm('{delete_confirm_bank}')" style="display:inline">
                            <button type="submit" class="btn-icon btn-icon-danger">{delete}</button>
                        </form>
                    </div>
                </div>"#,
                bank = a.bank_name,
                id = a.id,
                archived = t.acc_archived,
                unarchive = t.acc_unarchive,
                delete = t.acc_delete,
                delete_confirm_bank = t.acc_delete_confirm_bank,
            ));
        } else {
            let is_expired = session_expires_at < today;
            let expiry_class = if is_expired {
                "expiry-expired"
            } else if session_expires_at < warn_threshold {
                "expiry-warning"
            } else {
                "expiry-ok"
            };
            let expiry_label = if is_expired {
                t.acc_expired
            } else {
                t.acc_active
            };

            let reauth_btn = if is_expired || session_expires_at < warn_threshold {
                format!(
                    r#"<form method="POST" action="{base}/leanfin/accounts/{}/reauth" style="display:inline">
                        <button type="submit" class="btn-icon">{reauthorize}</button>
                    </form>"#,
                    a.id,
                    reauthorize = t.acc_reauthorize,
                )
            } else {
                String::new()
            };

            bank_items.push_str(&format!(
                r#"<div class="account-item">
                    <div>
                        <div class="account-bank">{}</div>
                        <div class="account-iban">{iban}</div>
                        {balance_html}
                    </div>
                    <div class="account-actions">
                        <span class="account-expiry {expiry_class}">{expiry_label} — {expires}</span>
                        {reauth_btn}
                        <form method="POST" action="{base}/leanfin/accounts/{id}/archive" style="display:inline">
                            <button type="submit" class="btn-icon">{archive}</button>
                        </form>
                        <form method="POST" action="{base}/leanfin/accounts/{id}/delete"
                              onsubmit="return confirm('{delete_confirm_bank}')" style="display:inline">
                            <button type="submit" class="btn-icon btn-icon-danger">{delete}</button>
                        </form>
                    </div>
                </div>"#,
                a.bank_name,
                id = a.id,
                archive = t.acc_archive,
                delete = t.acc_delete,
                delete_confirm_bank = t.acc_delete_confirm_bank,
            ));
        }
    }

    if bank_items.is_empty() {
        bank_items = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.acc_no_bank);
    }

    // Manual accounts section
    let mut manual_items = String::new();
    for a in &manual_accounts {
        let name = a.account_name.as_deref().unwrap_or(&a.bank_name);
        let balance_html = format_balance(a.balance_amount, a.balance_currency.as_deref());
        let category_badge = match a.asset_category.as_deref() {
            Some(cat) => format!(r#"<span class="category-badge">{cat}</span>"#),
            None => String::new(),
        };

        if a.archived {
            manual_items.push_str(&format!(
                r#"<div class="account-item account-archived">
                    <div>
                        <div class="account-bank">{name} <span class="archived-badge">{archived}</span></div>
                        {category_badge}
                        {balance_html}
                    </div>
                    <div class="account-actions">
                        <form method="POST" action="{base}/leanfin/accounts/{id}/unarchive" style="display:inline">
                            <button type="submit" class="btn-icon">{unarchive}</button>
                        </form>
                        <form method="POST" action="{base}/leanfin/accounts/{id}/delete"
                              onsubmit="return confirm('{delete_confirm_manual}')" style="display:inline">
                            <button type="submit" class="btn-icon btn-icon-danger">{delete}</button>
                        </form>
                    </div>
                </div>"#,
                id = a.id,
                archived = t.acc_archived,
                unarchive = t.acc_unarchive,
                delete = t.acc_delete,
                delete_confirm_manual = t.acc_delete_confirm_manual,
            ));
        } else {
            manual_items.push_str(&format!(
                r#"<div class="account-item">
                    <div>
                        <div class="account-bank">{name}</div>
                        {category_badge}
                        {balance_html}
                    </div>
                    <div class="account-actions">
                        <a href="{base}/leanfin/accounts/manual/{id}/value" class="btn-icon">{update_value}</a>
                        <a href="{base}/leanfin/accounts/manual/{id}/import-csv" class="btn-icon">{import_csv}</a>
                        <a href="{base}/leanfin/accounts/manual/{id}/edit" class="btn-icon">{edit}</a>
                        <form method="POST" action="{base}/leanfin/accounts/{id}/archive" style="display:inline">
                            <button type="submit" class="btn-icon">{archive}</button>
                        </form>
                        <form method="POST" action="{base}/leanfin/accounts/{id}/delete"
                              onsubmit="return confirm('{delete_confirm_manual}')" style="display:inline">
                            <button type="submit" class="btn-icon btn-icon-danger">{delete}</button>
                        </form>
                    </div>
                </div>"#,
                id = a.id,
                update_value = t.acc_update_value,
                import_csv = t.acc_import_csv,
                edit = t.acc_edit,
                archive = t.acc_archive,
                delete = t.acc_delete,
                delete_confirm_manual = t.acc_delete_confirm_manual,
            ));
        }
    }

    if manual_items.is_empty() {
        manual_items = format!(
            r#"<div class="empty-state"><p>{}</p></div>"#,
            t.acc_no_manual
        );
    }

    let archived_toggle = if has_archived {
        let checked = if show_archived { " checked" } else { "" };
        format!(
            r#"<label class="txn-filter-check" style="margin-left:auto">
                <input type="checkbox" id="show-archived"{checked}
                       onchange="window.location.href='{base}/leanfin/accounts' + (this.checked ? '?show_archived=1' : '')">
                {show_archived_label}
            </label>"#,
            show_archived_label = t.acc_show_archived,
        )
    } else {
        String::new()
    };

    let has_banking_creds = settings::has_credentials(&state.pool, user_id.0).await;
    let link_btn = if has_banking_creds {
        format!(
            r#"<a href="{base}/leanfin/accounts/link" class="btn btn-primary">{}</a>"#,
            t.acc_link
        )
    } else {
        format!(
            r#"<a href="{base}/leanfin/settings" class="btn btn-secondary">{}</a>"#,
            t.acc_configure_eb
        )
    };

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
            {archived_toggle}
        </div>
        {error_banner}
        <div class="card">
            <div class="card-header">
                <h2>{bank_accounts_heading}</h2>
                {link_btn}
            </div>
            <div class="card-body">
                <div class="account-grid">{bank_items}</div>
            </div>
        </div>
        <div class="card">
            <div class="card-header">
                <h2>{manual_accounts_heading}</h2>
                <a href="{base}/leanfin/accounts/manual/new" class="btn btn-primary">{add_account}</a>
            </div>
            <div class="card-body">
                <div class="account-grid">{manual_items}</div>
            </div>
        </div>"##,
        title = t.acc_title,
        subtitle = t.acc_subtitle,
        bank_accounts_heading = t.acc_bank_accounts,
        manual_accounts_heading = t.acc_manual_accounts,
        add_account = t.acc_add,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.accounts),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
}

fn format_balance(amount: Option<f64>, currency: Option<&str>) -> String {
    match (amount, currency) {
        (Some(amt), Some(cur)) => {
            let sign = if amt < 0.0 { "negative" } else { "positive" };
            format!(r#"<div class="account-balance {sign}">{amt:.2} {cur}</div>"#)
        }
        _ => String::new(),
    }
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: i64,
    bank_name: String,
    iban: Option<String>,
    session_expires_at: NaiveDateTime,
    balance_amount: Option<f64>,
    balance_currency: Option<String>,
    account_type: String,
    account_name: Option<String>,
    asset_category: Option<String>,
    archived: bool,
}

// ── Manual account: new ──────────────────────────────────────────

async fn manual_new_form(
    state: axum::extract::State<AppState>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);
    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/manual/new">
                    <label for="name">{name_label}</label>
                    <input type="text" id="name" name="name" required placeholder="e.g. Stock Portfolio">
                    <label for="category">{category_label}</label>
                    <select id="category" name="category">
                        <option value="investment">{cat_investment}</option>
                        <option value="real_estate">{cat_real_estate}</option>
                        <option value="vehicle">{cat_vehicle}</option>
                        <option value="loan">{cat_loan}</option>
                        <option value="crypto">{cat_crypto}</option>
                        <option value="other">{cat_other}</option>
                    </select>
                    <label for="currency">{currency_label}</label>
                    <input type="text" id="currency" name="currency" required maxlength="3"
                           pattern="[A-Z]{{3}}" placeholder="EUR" value="EUR" style="text-transform:uppercase">
                    <label for="initial_value">{initial_label}</label>
                    <input type="number" id="initial_value" name="initial_value" required step="0.01" placeholder="0.00">
                    <label for="date">{date_label}</label>
                    <input type="date" id="date" name="date" required>
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1">{add_btn}</button>
                    </div>
                </form>
            </div>
        </div>
        <script>document.getElementById('date').valueAsDate = new Date();</script>"#,
        title = t.acc_manual_new_title,
        subtitle = t.acc_manual_new_subtitle,
        name_label = t.acc_manual_name,
        category_label = t.acc_manual_category,
        cat_investment = t.acc_cat_investment,
        cat_real_estate = t.acc_cat_real_estate,
        cat_vehicle = t.acc_cat_vehicle,
        cat_loan = t.acc_cat_loan,
        cat_crypto = t.acc_cat_crypto,
        cat_other = t.acc_cat_other,
        currency_label = t.acc_manual_currency,
        initial_label = t.acc_manual_initial,
        date_label = t.acc_manual_date,
        cancel = t.acc_manual_cancel,
        add_btn = t.acc_manual_add_btn,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_manual_new_title),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct ManualNewForm {
    name: String,
    category: String,
    currency: String,
    initial_value: f64,
    date: String,
}

async fn manual_new_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<ManualNewForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let currency = form.currency.to_uppercase();

    let uid = format!("manual_{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        r#"INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type, account_name, asset_category, balance_amount, balance_currency)
           VALUES (?, ?, '', '', ?, '9999-12-31T00:00:00Z', 'manual', ?, ?, ?, ?)"#,
    )
    .bind(user_id.0)
    .bind(&form.name)
    .bind(&uid)
    .bind(&form.name)
    .bind(&form.category)
    .bind(form.initial_value)
    .bind(&currency)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) => {
            let account_id = r.last_insert_rowid();
            // Record initial balance snapshot
            let timestamp = format!("{}T23:59:59Z", &form.date);
            let _ = sqlx::query(
                r#"INSERT OR REPLACE INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
                   VALUES (?, ?, ?, ?, 'MANUAL')"#,
            )
            .bind(account_id)
            .bind(&timestamp)
            .bind(&form.date)
            .bind(form.initial_value)
            .execute(&state.pool)
            .await;

            tracing::info!(
                "Created manual account '{}' for user {}",
                form.name,
                user_id.0
            );
        }
        Err(e) => {
            tracing::error!("Failed to create manual account: {e}");
        }
    }

    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

// ── Manual account: edit ─────────────────────────────────────────

async fn manual_edit_form(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let account: Option<ManualAccountRow> = sqlx::query_as(
        "SELECT id, account_name, asset_category FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(account) = account else {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    };

    let name = account.account_name.as_deref().unwrap_or("");
    let category = account.asset_category.as_deref().unwrap_or("other");

    let cat_labels = [
        ("investment", t.acc_cat_investment),
        ("real_estate", t.acc_cat_real_estate),
        ("vehicle", t.acc_cat_vehicle),
        ("loan", t.acc_cat_loan),
        ("crypto", t.acc_cat_crypto),
        ("other", t.acc_cat_other),
    ];

    let category_options = cat_labels
        .iter()
        .map(|(value, label)| {
            let selected = if *value == category { " selected" } else { "" };
            format!(r#"<option value="{value}"{selected}>{label}</option>"#)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/manual/{id}/edit">
                    <label for="name">{name_label}</label>
                    <input type="text" id="name" name="name" required value="{name}">
                    <label for="category">{category_label}</label>
                    <select id="category" name="category">
                        {category_options}
                    </select>
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1">{save}</button>
                    </div>
                </form>
            </div>
        </div>"#,
        id = account.id,
        title = t.acc_edit_title,
        subtitle = t.acc_edit_subtitle,
        name_label = t.acc_manual_name,
        category_label = t.acc_manual_category,
        cancel = t.acc_manual_cancel,
        save = t.acc_save_changes,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_edit_title),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
    .into_response()
}

#[derive(sqlx::FromRow)]
struct ManualAccountRow {
    id: i64,
    account_name: Option<String>,
    asset_category: Option<String>,
}

#[derive(Deserialize)]
struct ManualEditForm {
    name: String,
    category: String,
}

async fn manual_edit_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
    Form(form): Form<ManualEditForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let _ = sqlx::query(
        "UPDATE leanfin_accounts SET account_name = ?, bank_name = ?, asset_category = ? WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0",
    )
    .bind(&form.name)
    .bind(&form.name)
    .bind(&form.category)
    .bind(account_id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await;

    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

// ── Manual account: update value ─────────────────────────────────

async fn manual_value_form(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let account: Option<ManualValueRow> = sqlx::query_as(
        "SELECT id, account_name, balance_amount, balance_currency FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(account) = account else {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    };

    let name = account.account_name.as_deref().unwrap_or("Account");
    let current = account
        .balance_amount
        .map(|v| format!("{v:.2}"))
        .unwrap_or_default();
    let currency = account.balance_currency.as_deref().unwrap_or("EUR");

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>Record a new value for {name}</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/manual/{id}/value">
                    <label for="value">{value_new} ({currency})</label>
                    <input type="number" id="value" name="value" required step="0.01" value="{current}">
                    <label for="date">{date_label}</label>
                    <input type="date" id="date" name="date" required>
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1">{record}</button>
                    </div>
                </form>
            </div>
        </div>
        <script>document.getElementById('date').valueAsDate = new Date();</script>"#,
        id = account.id,
        title = t.acc_value_title,
        value_new = t.acc_value_new,
        date_label = t.acc_value_date,
        cancel = t.acc_manual_cancel,
        record = t.acc_value_record,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_value_title),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
    .into_response()
}

#[derive(sqlx::FromRow)]
struct ManualValueRow {
    id: i64,
    account_name: Option<String>,
    balance_amount: Option<f64>,
    balance_currency: Option<String>,
}

#[derive(Deserialize)]
struct ManualValueForm {
    value: f64,
    date: String,
}

async fn manual_value_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
    Form(form): Form<ManualValueForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Verify ownership and account_type
    let owns: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0)",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if !owns {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    }

    // Update account balance
    let _ = sqlx::query("UPDATE leanfin_accounts SET balance_amount = ? WHERE id = ?")
        .bind(form.value)
        .bind(account_id)
        .execute(&state.pool)
        .await;

    // Upsert balance snapshot (delete same-day MANUAL, then insert)
    let timestamp = format!("{}T23:59:59Z", &form.date);
    let _ = sqlx::query(
        "DELETE FROM leanfin_balance_snapshots WHERE account_id = ? AND balance_type = 'MANUAL' AND date = ?",
    )
    .bind(account_id)
    .bind(&form.date)
    .execute(&state.pool)
    .await;
    let _ = sqlx::query(
        r#"INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
           VALUES (?, ?, ?, ?, 'MANUAL')"#,
    )
    .bind(account_id)
    .bind(&timestamp)
    .bind(&form.date)
    .bind(form.value)
    .execute(&state.pool)
    .await;

    tracing::info!(
        "Updated manual account {account_id} value to {}",
        form.value
    );

    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

// ── Manual account: import CSV ───────────────────────────────────

async fn import_csv_form(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let account: Option<ManualValueRow> = sqlx::query_as(
        "SELECT id, account_name, balance_amount, balance_currency FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(account) = account else {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    };

    let name = account.account_name.as_deref().unwrap_or("Account");

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>Bulk-import historical values for {name}</p>
        </div>
        <div class="card" style="max-width: 32rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/manual/{id}/import-csv" enctype="multipart/form-data">
                    <label for="file">{csv_file}</label>
                    <input type="file" id="file" name="file" accept=".csv" required>
                    <div class="csv-format-help" style="margin:1rem 0; padding:0.75rem; background:var(--surface-secondary, #f5f5f5); border-radius:0.375rem; font-size:0.875rem;">
                        <strong>{csv_format}</strong>
                        <pre style="margin:0.5rem 0 0;">date,value
2025-01-01,15000.00
2025-02-01,15250.50</pre>
                        <p style="margin:0.5rem 0 0;">{csv_format_desc}</p>
                    </div>
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1">{upload}</button>
                    </div>
                </form>
            </div>
        </div>"#,
        id = account.id,
        title = t.acc_csv_title,
        csv_file = t.acc_csv_file,
        csv_format = t.acc_csv_format,
        csv_format_desc = t.acc_csv_format_desc,
        cancel = t.acc_manual_cancel,
        upload = t.acc_csv_upload,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_csv_title),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
    .into_response()
}

async fn import_csv_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(account_id): Path<i64>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    // Verify ownership
    let account: Option<ManualValueRow> = sqlx::query_as(
        "SELECT id, account_name, balance_amount, balance_currency FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'manual' AND archived = 0",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(account) = account else {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    };

    let name = account.account_name.as_deref().unwrap_or("Account");

    // Extract file from multipart
    let mut csv_bytes: Option<Vec<u8>> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            match field.bytes().await {
                Ok(bytes) => csv_bytes = Some(bytes.to_vec()),
                Err(e) => {
                    return render_import_error(
                        &state.config,
                        name,
                        account_id,
                        &format!("Failed to read file: {e}"),
                        lang,
                    )
                    .into_response();
                }
            }
        }
    }

    let Some(csv_bytes) = csv_bytes else {
        return render_import_error(&state.config, name, account_id, "No file uploaded", lang)
            .into_response();
    };

    match super::services::csv_import::import_csv_balances(&state.pool, account_id, &csv_bytes)
        .await
    {
        Ok(result) if !result.skipped.is_empty() => {
            // Validation errors — show all problems
            let mut error_list = String::new();
            for s in &result.skipped {
                error_list.push_str(&format!(
                    "<li>Line {}: {}</li>",
                    s.line,
                    html_escape(&s.reason)
                ));
            }

            let body = format!(
                r#"<div class="page-header">
                    <h1>{import_failed}</h1>
                    <p>{fix_errors}</p>
                </div>
                <div class="card" style="max-width: 32rem;">
                    <div class="card-body">
                        <div class="alert alert-error">
                            <strong>{count} error(s) found — no rows were imported.</strong>
                            <ul style="margin:0.5rem 0 0; padding-left:1.25rem;">{error_list}</ul>
                        </div>
                        <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                            <a href="{base}/leanfin/accounts/manual/{id}/import-csv" class="btn btn-secondary">{try_again}</a>
                            <a href="{base}/leanfin/accounts" class="btn btn-secondary">{back}</a>
                        </div>
                    </div>
                </div>"#,
                count = result.skipped.len(),
                id = account_id,
                import_failed = t.acc_csv_import_failed,
                fix_errors = t.acc_csv_fix_errors,
                try_again = t.acc_csv_try_again,
                back = t.acc_csv_back,
            );
            Html(render_page(
                &format!("LeanFin — {}", t.acc_csv_import_failed),
                &leanfin_nav(base, "accounts", lang),
                &body,
                &state.config,
                lang,
            ))
            .into_response()
        }
        Ok(result) => {
            // Success
            let balance_info = match result.latest_balance {
                Some((date, val)) => format!("Latest value: {val:.2} as of {date}"),
                None => String::new(),
            };

            let body = format!(
                r#"<div class="page-header">
                    <h1>{import_complete}</h1>
                    <p>Successfully imported values for {name}</p>
                </div>
                <div class="card" style="max-width: 32rem;">
                    <div class="card-body">
                        <div class="alert alert-success">
                            <strong>{count} row(s) imported.</strong>
                            <p style="margin:0.25rem 0 0;">{balance_info}</p>
                        </div>
                        <div style="margin-top:1rem;">
                            <a href="{base}/leanfin/accounts" class="btn btn-secondary">{back}</a>
                        </div>
                    </div>
                </div>"#,
                count = result.imported,
                import_complete = t.acc_csv_import_complete,
                back = t.acc_csv_back,
            );

            tracing::info!(
                "Imported {count} CSV rows for manual account {account_id}",
                count = result.imported
            );
            Html(render_page(
                &format!("LeanFin — {}", t.acc_csv_import_complete),
                &leanfin_nav(base, "accounts", lang),
                &body,
                &state.config,
                lang,
            ))
            .into_response()
        }
        Err(e) => render_import_error(&state.config, name, account_id, &e.to_string(), lang)
            .into_response(),
    }
}

fn render_import_error(
    config: &crate::config::Config,
    name: &str,
    account_id: i64,
    error: &str,
    lang: Lang,
) -> Html<String> {
    let base = &config.base_path;
    let t = super::i18n::t(lang);
    let body = format!(
        r#"<div class="page-header">
            <h1>{import_failed}</h1>
            <p>Could not import values for {name}</p>
        </div>
        <div class="card" style="max-width: 32rem;">
            <div class="card-body">
                <div class="alert alert-error">{error}</div>
                <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                    <a href="{base}/leanfin/accounts/manual/{id}/import-csv" class="btn btn-secondary">{try_again}</a>
                    <a href="{base}/leanfin/accounts" class="btn btn-secondary">{back}</a>
                </div>
            </div>
        </div>"#,
        id = account_id,
        error = html_escape(error),
        import_failed = t.acc_csv_import_failed,
        try_again = t.acc_csv_try_again,
        back = t.acc_csv_back,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_csv_import_failed),
        &leanfin_nav(base, "accounts", lang),
        &body,
        config,
        lang,
    ))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Link: choose bank ─────────────────────────────────────────────

async fn link_form(
    state: axum::extract::State<AppState>,
    Extension(lang): Extension<Lang>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);
    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/link">
                    <label for="country">{country_label}</label>
                    <input type="text" id="country" name="country" required maxlength="2"
                           pattern="[A-Z]{{2}}" placeholder="ES" style="text-transform:uppercase">
                    <label for="bank_name">{bank_name_label}</label>
                    <input type="text" id="bank_name" name="bank_name" required placeholder="e.g. Santander">
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1">{connect}</button>
                    </div>
                </form>
            </div>
        </div>"#,
        title = t.acc_link_title,
        subtitle = t.acc_link_subtitle,
        country_label = t.acc_link_country,
        bank_name_label = t.acc_link_bank_name,
        cancel = t.acc_manual_cancel,
        connect = t.acc_link_connect,
    );
    Html(render_page(
        &format!("LeanFin — {}", t.acc_link_title),
        &leanfin_nav(base, "accounts", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct LinkForm {
    country: String,
    bank_name: String,
}

async fn link_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Form(form): Form<LinkForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let creds = match settings::get_credentials(&state.pool, &state.config, user_id.0).await {
        Ok(c) => c,
        Err(_) => {
            return Redirect::to(&format!("{base}/leanfin/settings")).into_response();
        }
    };

    let country = form.country.to_uppercase();
    let csrf_state = format!("{}:{}", user_id.0, uuid::Uuid::new_v4());

    // Store state in a pending_links table for CSRF validation on callback
    if let Err(e) = sqlx::query(
        "INSERT INTO leanfin_pending_links (state, user_id, bank_name, country) VALUES (?, ?, ?, ?)",
    )
    .bind(&csrf_state)
    .bind(user_id.0)
    .bind(&form.bank_name)
    .bind(&country)
    .execute(&state.pool)
    .await
    {
        tracing::error!("Failed to store pending link: {e}");
        return Html("Failed to start linking process".to_string()).into_response();
    }

    // Default to 90 days consent validity
    match enable_banking::start_auth(
        &state.pool,
        &creds,
        &form.bank_name,
        &country,
        &csrf_state,
        90,
    )
    .await
    {
        Ok(auth_resp) => Redirect::to(&auth_resp.url).into_response(),
        Err(e) => {
            tracing::error!("Enable Banking auth failed: {e:#}");
            Html(format!("Failed to connect to bank: {e}")).into_response()
        }
    }
}

// ── Re-authorize expired session ─────────────────────────────────

async fn reauth(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let creds = match settings::get_credentials(&state.pool, &state.config, user_id.0).await {
        Ok(c) => c,
        Err(_) => {
            return Redirect::to(&format!("{base}/leanfin/settings")).into_response();
        }
    };

    // Verify account belongs to this user and get bank details
    let account: Option<ReauthAccountRow> = sqlx::query_as(
        "SELECT id, bank_name, bank_country FROM leanfin_accounts WHERE id = ? AND user_id = ? AND account_type = 'bank'",
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(account) = account else {
        return Redirect::to(&format!("{base}/leanfin/accounts")).into_response();
    };

    let csrf_state = format!("{}:{}", user_id.0, uuid::Uuid::new_v4());

    // Store pending link with reauth_account_id so callback knows to update
    if let Err(e) = sqlx::query(
        "INSERT INTO leanfin_pending_links (state, user_id, bank_name, country, reauth_account_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&csrf_state)
    .bind(user_id.0)
    .bind(&account.bank_name)
    .bind(&account.bank_country)
    .bind(account.id)
    .execute(&state.pool)
    .await
    {
        tracing::error!("Failed to store pending reauth link: {e}");
        return Html("Failed to start re-authorization".to_string()).into_response();
    }

    match enable_banking::start_auth(
        &state.pool,
        &creds,
        &account.bank_name,
        &account.bank_country,
        &csrf_state,
        90,
    )
    .await
    {
        Ok(auth_resp) => Redirect::to(&auth_resp.url).into_response(),
        Err(e) => {
            tracing::error!("Enable Banking reauth failed: {e:#}");
            Html(format!("Failed to re-authorize: {e}")).into_response()
        }
    }
}

#[derive(sqlx::FromRow)]
struct ReauthAccountRow {
    id: i64,
    bank_name: String,
    bank_country: String,
}

// ── Archive / Unarchive account ──────────────────────────────────

async fn archive_account(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Check for unallocated transactions (bank accounts only — manual accounts have no transactions)
    let has_unallocated: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 FROM leanfin_transactions t
            JOIN leanfin_accounts a ON t.account_id = a.id
            WHERE a.id = ? AND a.user_id = ?
              AND t.id NOT IN (
                SELECT al.transaction_id FROM leanfin_allocations al
                GROUP BY al.transaction_id
                HAVING ABS(SUM(al.amount) - ABS(
                    (SELECT t2.amount FROM leanfin_transactions t2 WHERE t2.id = al.transaction_id)
                )) < 0.01
              )
        )"#,
    )
    .bind(account_id)
    .bind(user_id.0)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if has_unallocated {
        // Redirect back with an error — the UI will show an alert
        return Redirect::to(&format!(
            "{base}/leanfin/accounts?archive_error={account_id}"
        ))
        .into_response();
    }

    let _ = sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ? AND user_id = ?")
        .bind(account_id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await;

    tracing::info!("Archived account {account_id} for user {}", user_id.0);
    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

async fn unarchive_account(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let _ = sqlx::query("UPDATE leanfin_accounts SET archived = 0 WHERE id = ? AND user_id = ?")
        .bind(account_id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await;

    tracing::info!("Unarchived account {account_id} for user {}", user_id.0);
    Redirect::to(&format!("{base}/leanfin/accounts?show_archived=1")).into_response()
}

// ── Delete account ───────────────────────────────────────────────

async fn delete_account(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let result = sqlx::query("DELETE FROM leanfin_accounts WHERE id = ? AND user_id = ?")
        .bind(account_id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) => {
            if r.rows_affected() > 0 {
                tracing::info!("Deleted account {account_id} for user {}", user_id.0);
            }
        }
        Err(e) => tracing::error!("Failed to delete account {account_id}: {e}"),
    }

    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

// ── OAuth callback ────────────────────────────────────────────────

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    state: axum::extract::State<AppState>,
    Query(params): Query<CallbackQuery>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    // Validate CSRF state and get the pending link info
    let pending: Option<PendingLink> = sqlx::query_as(
        "SELECT state, user_id, bank_name, country, reauth_account_id FROM leanfin_pending_links WHERE state = ?",
    )
    .bind(&params.state)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(pending) = pending else {
        return Html("Invalid or expired authorization state".to_string()).into_response();
    };

    // Clean up the pending link
    let _ = sqlx::query("DELETE FROM leanfin_pending_links WHERE state = ?")
        .bind(&params.state)
        .execute(&state.pool)
        .await;

    // Fetch user credentials for the session exchange
    let creds = match settings::get_credentials(&state.pool, &state.config, pending.user_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(
                "Failed to get credentials for user {}: {e:#}",
                pending.user_id
            );
            return Html("Enable Banking credentials not configured".to_string()).into_response();
        }
    };

    // Exchange code for session
    let session = match enable_banking::create_session(&state.pool, &creds, &params.code).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to create Enable Banking session: {e:#}");
            return Html(format!("Failed to complete bank authorization: {e}")).into_response();
        }
    };

    // Parse session expiry
    let expires_at =
        chrono::NaiveDateTime::parse_from_str(&session.access.valid_until, "%Y-%m-%dT%H:%M:%SZ")
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(
                    &session.access.valid_until,
                    "%Y-%m-%dT%H:%M:%S%.fZ",
                )
            })
            .unwrap_or_else(|_| (chrono::Utc::now() + chrono::Duration::days(90)).naive_utc());

    if let Some(reauth_id) = pending.reauth_account_id {
        // Re-authorization: update existing accounts that share the same bank session
        let mut updated = 0u64;
        for account in &session.accounts {
            let result = sqlx::query(
                "UPDATE leanfin_accounts SET session_id = ?, session_expires_at = ? WHERE account_uid = ? AND user_id = ?",
            )
            .bind(&session.session_id)
            .bind(expires_at)
            .bind(&account.uid)
            .bind(pending.user_id)
            .execute(&state.pool)
            .await;

            match result {
                Ok(r) => updated += r.rows_affected(),
                Err(e) => tracing::error!("Failed to update account {}: {e}", account.uid),
            }
        }

        // If the specific account wasn't matched by UID (bank may assign new UIDs),
        // fall back to updating by the reauth account ID directly
        if updated == 0
            && let Some(first) = session.accounts.first()
        {
            let iban = first.account_id.as_ref().and_then(|id| id.iban.as_deref());
            let _ = sqlx::query(
                    "UPDATE leanfin_accounts SET session_id = ?, account_uid = ?, iban = COALESCE(?, iban), session_expires_at = ? WHERE id = ? AND user_id = ?",
                )
                .bind(&session.session_id)
                .bind(&first.uid)
                .bind(iban)
                .bind(expires_at)
                .bind(reauth_id)
                .bind(pending.user_id)
                .execute(&state.pool)
                .await;
        }

        tracing::info!(
            "Re-authorized {} account(s) from {} for user {}",
            updated.max(1),
            pending.bank_name,
            pending.user_id
        );
    } else {
        // New link: insert accounts
        let mut linked = 0;
        for account in &session.accounts {
            let iban = account
                .account_id
                .as_ref()
                .and_then(|id| id.iban.as_deref());

            let result = sqlx::query(
                r#"INSERT OR IGNORE INTO leanfin_accounts
                   (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, account_type)
                   VALUES (?, ?, ?, ?, ?, ?, ?, 'bank')"#,
            )
            .bind(pending.user_id)
            .bind(&pending.bank_name)
            .bind(&pending.country)
            .bind(iban)
            .bind(&session.session_id)
            .bind(&account.uid)
            .bind(expires_at)
            .execute(&state.pool)
            .await;

            match result {
                Ok(r) => linked += r.rows_affected(),
                Err(e) => tracing::error!("Failed to store account {}: {e}", account.uid),
            }
        }

        tracing::info!(
            "Linked {linked} account(s) from {} for user {}",
            pending.bank_name,
            pending.user_id
        );
    }

    Redirect::to(&format!("{base}/leanfin/accounts")).into_response()
}

#[derive(sqlx::FromRow)]
struct PendingLink {
    #[allow(dead_code)]
    state: String,
    user_id: i64,
    bank_name: String,
    country: String,
    reauth_account_id: Option<i64>,
}
