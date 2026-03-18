use anyhow::{Context, Result};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use chrono::Utc;
use rand::Rng;
use sqlx::SqlitePool;
use tower_cookies::Cookies;

use crate::models::user_settings;

const SESSION_COOKIE: &str = "session";
const SESSION_DURATION_DAYS: i64 = 30;

pub async fn create_user(pool: &SqlitePool, username: &str, password: &str) -> Result<i64> {
    let salt = SaltString::generate(&mut rand_core_06::OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?
        .to_string();

    let result =
        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?) RETURNING id")
            .bind(username)
            .bind(&password_hash)
            .fetch_one(pool)
            .await
            .context("failed to create user")?;

    Ok(sqlx::Row::get(&result, "id"))
}

pub async fn verify_password(pool: &SqlitePool, username: &str, password: &str) -> Result<i64> {
    let row = sqlx::query_as::<_, crate::models::User>(
        "SELECT id, username, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("invalid credentials"))?;

    let parsed_hash = PasswordHash::new(&row.password_hash)
        .map_err(|e| anyhow::anyhow!("invalid hash in database: {e}"))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| anyhow::anyhow!("invalid credentials"))?;

    Ok(row.id)
}

pub async fn create_session(pool: &SqlitePool, user_id: i64) -> Result<String> {
    let token: String = hex::encode(rand::rng().random::<[u8; 32]>());
    let expires_at = Utc::now().naive_utc() + chrono::Duration::days(SESSION_DURATION_DAYS);

    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(user_id)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(token)
}

pub async fn delete_session(pool: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_user_id_from_session(pool: &SqlitePool, token: &str) -> Result<Option<i64>> {
    let now = Utc::now().naive_utc();
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT user_id FROM sessions WHERE token = ? AND expires_at > ?")
            .bind(token)
            .bind(now)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|r| r.0))
}

/// Axum middleware that redirects to /login if the user is not authenticated.
/// Also injects `Lang` into request extensions based on user preferences.
pub async fn require_auth(
    cookies: Cookies,
    state: axum::extract::State<crate::routes::AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let has_cookie = cookies.get(SESSION_COOKIE).is_some();
    let authenticated = async {
        let cookie = cookies.get(SESSION_COOKIE)?;
        let user_id = get_user_id_from_session(&state.pool, cookie.value())
            .await
            .ok()
            .flatten()?;
        Some(user_id)
    }
    .await;

    match authenticated {
        Some(user_id) => {
            let lang = user_settings::get_language(&state.pool, user_id).await;
            request.extensions_mut().insert(UserId(user_id));
            request.extensions_mut().insert(lang);
            next.run(request).await
        }
        None => {
            tracing::warn!(
                "Auth failed for {} (cookie present: {has_cookie})",
                request.uri()
            );
            let login_url = format!("{}/login", state.config.base_path);
            Redirect::to(&login_url).into_response()
        }
    }
}

#[derive(Clone, Copy)]
pub struct UserId(pub i64);
