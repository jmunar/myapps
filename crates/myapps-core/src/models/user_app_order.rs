use sqlx::SqlitePool;

/// The user's saved launcher order, first card first. Empty if they never
/// reordered.
pub async fn get_order(pool: &SqlitePool, user_id: i64) -> Vec<String> {
    sqlx::query_scalar("SELECT app_key FROM user_app_order WHERE user_id = ? ORDER BY position")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("DB query failed: {e:#}");
            Default::default()
        })
}

/// Replace the user's launcher order with `keys`, in that order.
pub async fn set_order(pool: &SqlitePool, user_id: i64, keys: &[&str]) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM user_app_order WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    for (position, key) in keys.iter().enumerate() {
        sqlx::query("INSERT INTO user_app_order (user_id, app_key, position) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(key)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}
