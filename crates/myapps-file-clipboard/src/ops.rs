//! Shared action functions, callable from both HTTP handlers and the command bar.

use myapps_core::command::{CommandAction, CommandResult};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::storage;

/// Retention bounds. A clipboard is deliberately short-lived, but a year is a
/// reasonable ceiling for someone who wants one.
pub const MIN_RETENTION_DAYS: i64 = 1;
pub const MAX_RETENTION_DAYS: i64 = 365;

#[derive(sqlx::FromRow)]
pub struct FileRow {
    pub id: i64,
    pub original_name: String,
    pub size_bytes: i64,
    pub created_at: String,
    pub expires_at: String,
}

/// List a user's files, newest first. Expired-but-not-yet-swept rows are hidden
/// so the list stays truthful even if the retention sweep has not run.
pub async fn list_files(pool: &SqlitePool, user_id: i64) -> Result<Vec<FileRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, original_name, size_bytes, created_at, expires_at
         FROM file_clipboard_files
         WHERE user_id = ? AND expires_at > datetime('now')
         ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Total bytes a user currently holds, including rows awaiting the sweep — the
/// bytes are still on disk, so they still count against the quota.
pub async fn usage_bytes(pool: &SqlitePool, user_id: i64) -> Result<u64, sqlx::Error> {
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes), 0) FROM file_clipboard_files WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(total.unwrap_or(0).max(0) as u64)
}

/// A user's retention setting, falling back to the deployment default.
pub async fn retention_days(pool: &SqlitePool, user_id: i64, default_days: i64) -> i64 {
    let stored: Option<i64> =
        sqlx::query_scalar("SELECT retention_days FROM file_clipboard_settings WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    stored
        .unwrap_or(default_days)
        .clamp(MIN_RETENTION_DAYS, MAX_RETENTION_DAYS)
}

/// Store the retention setting and re-stamp existing files against it, so the
/// UI's "expires" column always reflects the period the user just chose.
pub async fn set_retention_days(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO file_clipboard_settings (user_id, retention_days) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET retention_days = excluded.retention_days",
    )
    .bind(user_id)
    .bind(days)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE file_clipboard_files SET expires_at = datetime(created_at, ?) WHERE user_id = ?",
    )
    .bind(format!("+{days} days"))
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Insert the metadata row for a file whose bytes are already on disk.
pub async fn record_file(
    pool: &SqlitePool,
    user_id: i64,
    original_name: &str,
    stored_name: &str,
    size_bytes: u64,
    content_type: Option<&str>,
    retention_days: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO file_clipboard_files
             (user_id, original_name, stored_name, size_bytes, content_type, expires_at)
         VALUES (?, ?, ?, ?, ?, datetime('now', ?))",
    )
    .bind(user_id)
    .bind(original_name)
    .bind(stored_name)
    .bind(size_bytes as i64)
    .bind(content_type)
    .bind(format!("+{retention_days} days"))
    .execute(pool)
    .await?;
    Ok(())
}

/// Look up the stored (on-disk) name of a file the user owns.
pub async fn stored_name(
    pool: &SqlitePool,
    user_id: i64,
    file_id: i64,
) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT stored_name, original_name FROM file_clipboard_files
         WHERE id = ? AND user_id = ? AND expires_at > datetime('now')",
    )
    .bind(file_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Delete a file: row first, then the bytes. If the process dies between the
/// two, the orphan sweep reclaims the bytes.
pub async fn delete_file(
    pool: &SqlitePool,
    base_dir: &str,
    user_id: i64,
    file_id: i64,
) -> Result<(), sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT stored_name FROM file_clipboard_files WHERE id = ? AND user_id = ?")
            .bind(file_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

    let Some((stored,)) = row else {
        return Ok(());
    };

    sqlx::query("DELETE FROM file_clipboard_files WHERE id = ? AND user_id = ?")
        .bind(file_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    storage::remove(base_dir, user_id, &stored).await;
    Ok(())
}

// ── Command bar ─────────────────────────────────────────────

pub fn commands() -> Vec<CommandAction> {
    vec![]
}

pub async fn dispatch(
    _pool: &SqlitePool,
    _user_id: i64,
    action: &str,
    _params: &HashMap<String, serde_json::Value>,
    _base_path: &str,
) -> Result<CommandResult, String> {
    Err(format!("Unknown FileClipboard action: {action}"))
}
