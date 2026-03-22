use sqlx::SqlitePool;
use std::collections::HashMap;

/// Returns visibility preferences for a user. Missing keys default to visible (true).
pub async fn get_visibility(pool: &SqlitePool, user_id: i64) -> HashMap<String, bool> {
    let rows: Vec<(String, i32)> =
        sqlx::query_as("SELECT app_key, visible FROM user_app_visibility WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    rows.into_iter().map(|(k, v)| (k, v != 0)).collect()
}

/// Upsert a single visibility preference.
pub async fn set_visibility(
    pool: &SqlitePool,
    user_id: i64,
    app_key: &str,
    visible: bool,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO user_app_visibility (user_id, app_key, visible) VALUES (?, ?, ?)
         ON CONFLICT(user_id, app_key) DO UPDATE SET visible = excluded.visible",
    )
    .bind(user_id)
    .bind(app_key)
    .bind(visible as i32)
    .execute(pool)
    .await?;
    Ok(())
}
