use axum::{
    Extension, Form, Router,
    extract::Query,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use sqlx::SqlitePool;

use super::AppState;
use crate::auth::UserId;
use crate::services::enable_banking;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/accounts", get(list_accounts))
        .route("/accounts/link", get(link_form).post(link_submit))
        .route("/accounts/callback", get(callback))
}

// ── List accounts ─────────────────────────────────────────────────

async fn list_accounts(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

    let accounts: Vec<AccountRow> = sqlx::query_as(
        "SELECT id, bank_name, iban, session_expires_at FROM accounts WHERE user_id = ?",
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
        let iban = a.iban.as_deref().unwrap_or("—");
        let expiry_class = if a.session_expires_at < today {
            "expiry-expired"
        } else if a.session_expires_at < warn_threshold {
            "expiry-warning"
        } else {
            "expiry-ok"
        };
        let expiry_label = if a.session_expires_at < today {
            "Expired"
        } else {
            "Active"
        };
        items.push_str(&format!(
            r#"<div class="account-item">
                <div>
                    <div class="account-bank">{}</div>
                    <div class="account-iban">{iban}</div>
                </div>
                <span class="account-expiry {expiry_class}">{expiry_label} — {expires}</span>
            </div>"#,
            a.bank_name
        ));
    }

    if items.is_empty() {
        items = r#"<div class="empty-state"><p>No accounts linked yet.</p></div>"#.into();
    }

    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LeanFin — Accounts</title>
    <link rel="stylesheet" href="{base}/static/style.css">
</head>
<body>
    <nav>
        <span class="brand">LeanFin</span>
        <a href="{base}/">Transactions</a>
        <a href="{base}/accounts" class="active">Accounts</a>
        <a href="{base}/labels">Labels</a>
        <a href="{base}/logout" class="nav-right">Log out</a>
    </nav>
    <main>
        <div class="page-header">
            <h1>Bank Accounts</h1>
            <p>Manage your linked bank connections</p>
        </div>
        <div class="card">
            <div class="card-header">
                <h2>Linked accounts</h2>
                <a href="{base}/accounts/link" class="btn btn-primary">+ Link account</a>
            </div>
            <div class="card-body">
                <div class="account-grid">{items}</div>
            </div>
        </div>
    </main>
</body>
</html>"#
    ))
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    #[allow(dead_code)]
    id: i64,
    bank_name: String,
    iban: Option<String>,
    session_expires_at: NaiveDateTime,
}

// ── Link: choose bank ─────────────────────────────────────────────

async fn link_form(
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LeanFin — Link Bank</title>
    <link rel="stylesheet" href="{base}/static/style.css">
</head>
<body>
    <nav>
        <span class="brand">LeanFin</span>
        <a href="{base}/">Transactions</a>
        <a href="{base}/accounts" class="active">Accounts</a>
        <a href="{base}/labels">Labels</a>
        <a href="{base}/logout" class="nav-right">Log out</a>
    </nav>
    <main>
        <div class="page-header">
            <h1>Link a bank account</h1>
            <p>Connect to your bank via Enable Banking (PSD2)</p>
        </div>
        <div class="card" style="max-width: 28rem;">
            <div class="card-body">
                <form method="POST" action="{base}/accounts/link">
                    <label for="country">Country code</label>
                    <input type="text" id="country" name="country" required maxlength="2"
                           pattern="[A-Z]{{2}}" placeholder="ES" style="text-transform:uppercase">
                    <label for="bank_name">Bank name</label>
                    <input type="text" id="bank_name" name="bank_name" required placeholder="e.g. Santander">
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/accounts" class="btn btn-secondary">Cancel</a>
                        <button type="submit" style="flex:1">Connect bank</button>
                    </div>
                </form>
            </div>
        </div>
    </main>
</body>
</html>"#
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
        "SELECT state, user_id, bank_name, country FROM pending_links WHERE state = ?",
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

    // Store each account from the session
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

    Redirect::to(&format!("{base}/accounts")).into_response()
}

#[derive(sqlx::FromRow)]
struct PendingLink {
    #[allow(dead_code)]
    state: String,
    user_id: i64,
    bank_name: String,
    country: String,
}
