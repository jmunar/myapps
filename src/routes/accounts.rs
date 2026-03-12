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

    let mut rows = String::new();
    for a in &accounts {
        let expires = a
            .session_expires_at
            .format("%Y-%m-%d")
            .to_string();
        let iban = a.iban.as_deref().unwrap_or("—");
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{iban}</td><td>{expires}</td></tr>",
            a.bank_name
        ));
    }

    if rows.is_empty() {
        rows = "<tr><td colspan=\"3\">No accounts linked yet.</td></tr>".into();
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
        <strong>LeanFin</strong>
        <a href="{base}/">Dashboard</a>
        <a href="{base}/logout">Log out</a>
    </nav>
    <main>
        <h1>Bank Accounts</h1>
        <table>
            <thead><tr><th>Bank</th><th>IBAN</th><th>Session expires</th></tr></thead>
            <tbody>{rows}</tbody>
        </table>
        <p style="margin-top:1rem"><a href="{base}/accounts/link">+ Link a new account</a></p>
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
        <strong>LeanFin</strong>
        <a href="{base}/accounts">Accounts</a>
        <a href="{base}/logout">Log out</a>
    </nav>
    <main>
        <h1>Link a bank account</h1>
        <form method="POST" action="{base}/accounts/link">
            <label for="country">Country code (e.g. ES, DE, FI)</label>
            <input type="text" id="country" name="country" required maxlength="2"
                   pattern="[A-Z]{{2}}" placeholder="ES" style="text-transform:uppercase">
            <label for="bank_name">Bank name (exact, as listed by Enable Banking)</label>
            <input type="text" id="bank_name" name="bank_name" required placeholder="e.g. Santander">
            <button type="submit">Connect</button>
        </form>
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
