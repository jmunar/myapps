use axum::{
    Extension, Form, Router,
    extract::{Path, Query},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::routes::AppState;
use crate::auth::UserId;
use super::services::enable_banking;
use super::dashboard::leanfin_nav;
use super::sync_handler::sync_button;
use crate::layout::render_page;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list_accounts))
        .route("/accounts/link", get(link_form).post(link_submit))
        .route("/accounts/callback", get(callback))
        .route("/accounts/{id}/reauth", post(reauth))
        .route("/accounts/{id}/delete", post(delete_account))
}

// ── List accounts ─────────────────────────────────────────────────

async fn list_accounts(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

    let accounts: Vec<AccountRow> = sqlx::query_as(
        "SELECT id, bank_name, iban, session_expires_at, balance_amount, balance_currency FROM accounts WHERE user_id = ?",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let today = chrono::Utc::now().naive_utc();
    let warn_threshold = today + chrono::Duration::days(14);

    let mut items = String::new();
    for a in &accounts {
        let expires = a.session_expires_at.format("%Y-%m-%d").to_string();
        let iban = a.iban.as_deref().unwrap_or("\u{2014}");
        let balance_html = match (a.balance_amount, a.balance_currency.as_deref()) {
            (Some(amt), Some(cur)) => {
                let sign = if amt < 0.0 { "negative" } else { "positive" };
                format!(r#"<div class="account-balance {sign}">{amt:.2} {cur}</div>"#)
            }
            _ => String::new(),
        };
        let is_expired = a.session_expires_at < today;
        let expiry_class = if is_expired {
            "expiry-expired"
        } else if a.session_expires_at < warn_threshold {
            "expiry-warning"
        } else {
            "expiry-ok"
        };
        let expiry_label = if is_expired { "Expired" } else { "Active" };

        let reauth_btn = if is_expired || a.session_expires_at < warn_threshold {
            format!(
                r#"<form method="POST" action="{base}/leanfin/accounts/{}/reauth" style="display:inline">
                    <button type="submit" class="btn-icon">Re-authorize</button>
                </form>"#,
                a.id
            )
        } else {
            String::new()
        };

        items.push_str(&format!(
            r#"<div class="account-item">
                <div>
                    <div class="account-bank">{}</div>
                    <div class="account-iban">{iban}</div>
                    {balance_html}
                </div>
                <div class="account-actions">
                    <span class="account-expiry {expiry_class}">{expiry_label} — {expires}</span>
                    {reauth_btn}
                    <form method="POST" action="{base}/leanfin/accounts/{}/delete"
                          onsubmit="return confirm('Delete this account and all its transactions?')" style="display:inline">
                        <button type="submit" class="btn-icon btn-icon-danger">Delete</button>
                    </form>
                </div>
            </div>"#,
            a.bank_name, a.id
        ));
    }

    if items.is_empty() {
        items = r#"<div class="empty-state"><p>No accounts linked yet.</p></div>"#.into();
    }

    let sync_btn = sync_button(base);
    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>Bank Accounts</h1>
                <div class="sync-container" id="sync-container">
                    {sync_btn}
                </div>
            </div>
            <p>Manage your linked bank connections</p>
        </div>
        <div class="card">
            <div class="card-header">
                <h2>Linked accounts</h2>
                <a href="{base}/leanfin/accounts/link" class="btn btn-primary">+ Link account</a>
            </div>
            <div class="card-body">
                <div class="account-grid">{items}</div>
            </div>
        </div>"##
    );

    Html(render_page("LeanFin — Accounts", &leanfin_nav(base, "accounts"), &body, base))
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: i64,
    bank_name: String,
    iban: Option<String>,
    session_expires_at: NaiveDateTime,
    balance_amount: Option<f64>,
    balance_currency: Option<String>,
}

// ── Link: choose bank ─────────────────────────────────────────────

async fn link_form(
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let body = format!(
        r#"<div class="page-header">
            <h1>Link a bank account</h1>
            <p>Connect to your bank via Enable Banking (PSD2)</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/accounts/link">
                    <label for="country">Country code</label>
                    <input type="text" id="country" name="country" required maxlength="2"
                           pattern="[A-Z]{{2}}" placeholder="ES" style="text-transform:uppercase">
                    <label for="bank_name">Bank name</label>
                    <input type="text" id="bank_name" name="bank_name" required placeholder="e.g. Santander">
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin/accounts" class="btn btn-secondary">Cancel</a>
                        <button type="submit" style="flex:1">Connect bank</button>
                    </div>
                </form>
            </div>
        </div>"#
    );
    Html(render_page("LeanFin — Link Bank", &leanfin_nav(base, "accounts"), &body, base))
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
    let country = form.country.to_uppercase();
    let csrf_state = format!("{}:{}", user_id.0, uuid::Uuid::new_v4());

    // Store state in a pending_links table for CSRF validation on callback
    if let Err(e) = sqlx::query(
        "INSERT INTO pending_links (state, user_id, bank_name, country) VALUES (?, ?, ?, ?)",
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
    match enable_banking::start_auth(&state.config, &form.bank_name, &country, &csrf_state, 90)
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

    // Verify account belongs to this user and get bank details
    let account: Option<ReauthAccountRow> = sqlx::query_as(
        "SELECT id, bank_name, bank_country FROM accounts WHERE id = ? AND user_id = ?",
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
        "INSERT INTO pending_links (state, user_id, bank_name, country, reauth_account_id) VALUES (?, ?, ?, ?, ?)",
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
        &state.config,
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

// ── Delete account ───────────────────────────────────────────────

async fn delete_account(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(account_id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;

    let result = sqlx::query("DELETE FROM accounts WHERE id = ? AND user_id = ?")
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
        "SELECT state, user_id, bank_name, country, reauth_account_id FROM pending_links WHERE state = ?",
    )
    .bind(&params.state)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(pending) = pending else {
        return Html("Invalid or expired authorization state".to_string()).into_response();
    };

    // Clean up the pending link
    let _ = sqlx::query("DELETE FROM pending_links WHERE state = ?")
        .bind(&params.state)
        .execute(&state.pool)
        .await;

    // Exchange code for session
    let session = match enable_banking::create_session(&state.config, &params.code).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to create Enable Banking session: {e:#}");
            return Html(format!("Failed to complete bank authorization: {e}")).into_response();
        }
    };

    // Parse session expiry
    let expires_at = chrono::NaiveDateTime::parse_from_str(
        &session.access.valid_until,
        "%Y-%m-%dT%H:%M:%SZ",
    )
    .or_else(|_| {
        chrono::NaiveDateTime::parse_from_str(
            &session.access.valid_until,
            "%Y-%m-%dT%H:%M:%S%.fZ",
        )
    })
    .unwrap_or_else(|_| {
        (chrono::Utc::now() + chrono::Duration::days(90)).naive_utc()
    });

    if let Some(reauth_id) = pending.reauth_account_id {
        // Re-authorization: update existing accounts that share the same bank session
        // The new session may return the same or different account UIDs, so update
        // all accounts from this bank for this user that match one of the new UIDs.
        let mut updated = 0u64;
        for account in &session.accounts {
            let result = sqlx::query(
                "UPDATE accounts SET session_id = ?, session_expires_at = ? WHERE account_uid = ? AND user_id = ?",
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
        if updated == 0 {
            if let Some(first) = session.accounts.first() {
                let iban = first.account_id.as_ref().and_then(|id| id.iban.as_deref());
                let _ = sqlx::query(
                    "UPDATE accounts SET session_id = ?, account_uid = ?, iban = COALESCE(?, iban), session_expires_at = ? WHERE id = ? AND user_id = ?",
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
                r#"INSERT OR IGNORE INTO accounts
                   (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#,
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
