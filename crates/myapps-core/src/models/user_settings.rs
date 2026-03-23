use crate::i18n::Lang;
use sqlx::SqlitePool;

/// Get the user's preferred language, defaulting to English.
pub async fn get_language(pool: &SqlitePool, user_id: i64) -> Lang {
    let code: Option<String> =
        sqlx::query_scalar("SELECT language FROM user_settings WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

    code.map(|c| Lang::from_code(&c)).unwrap_or_default()
}

/// Set the user's preferred language.
pub async fn set_language(pool: &SqlitePool, user_id: i64, lang: Lang) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO user_settings (user_id, language) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET language = excluded.language",
    )
    .bind(user_id)
    .bind(lang.code())
    .execute(pool)
    .await?;
    Ok(())
}
