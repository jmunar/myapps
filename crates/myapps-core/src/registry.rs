use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use sqlx::SqlitePool;

use crate::command::{CommandAction, CommandResult};
use crate::config::Config;
use crate::routes::AppState;

/// Metadata for an application in the launcher.
pub struct AppInfo {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    pub path: &'static str,
}

/// Trait that every sub-application implements.
pub trait App: Send + Sync {
    /// Static metadata (key, name, icon, launcher path).
    fn info(&self) -> AppInfo;

    /// Translated app description for the launcher.
    fn description(&self, lang: crate::i18n::Lang) -> &'static str;

    /// Axum router, nested under `info().path`.
    fn router(&self) -> Router<AppState>;

    /// Command-bar actions this app exposes.
    fn commands(&self) -> Vec<CommandAction> {
        vec![]
    }

    /// Execute a command-bar action.
    fn dispatch<'a>(
        &'a self,
        _pool: &'a SqlitePool,
        _user_id: i64,
        _action: &'a str,
        _params: &'a HashMap<String, serde_json::Value>,
        _base_path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<CommandResult, String>> + Send + 'a>> {
        Box::pin(async { Err("not implemented".into()) })
    }

    /// Dynamic context for the LLM command prompt.
    fn command_context<'a>(
        &'a self,
        _pool: &'a SqlitePool,
        _user_id: i64,
    ) -> Pin<Box<dyn Future<Output = HashMap<String, String>> + Send + 'a>> {
        Box::pin(async { HashMap::new() })
    }

    /// Seed demo data for a new user. Returns `None` if the app has no seed.
    fn seed<'a>(
        &'a self,
        _pool: &'a SqlitePool,
        _user_id: i64,
    ) -> Option<Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>> {
        None
    }

    /// Scheduled task invoked by `myapps cron` (e.g. daily via system cron).
    fn cron<'a>(
        &'a self,
        _pool: &'a SqlitePool,
        _config: &'a Config,
    ) -> Option<Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>> {
        None
    }

    /// App-specific CSS (embedded at compile time via `include_str!`).
    fn css(&self) -> &'static str {
        ""
    }

    /// App-specific database migrations (embedded at compile time).
    fn migrations(&self) -> sqlx::migrate::Migrator {
        sqlx::migrate::Migrator::DEFAULT
    }

    /// Hook called on `serve` to start background workers.
    fn on_serve(&self, _pool: SqlitePool, _config: Arc<Config>) {}
}

/// Delete all data owned by `app` for a single user.
///
/// Discovers tables whose name starts with the app key followed by `_` and that
/// have a `user_id` column, then deletes all rows for the given user. Existing
/// `ON DELETE CASCADE` foreign keys handle child rows automatically.
pub async fn delete_user_app_data(
    pool: &SqlitePool,
    app: &dyn App,
    user_id: i64,
) -> anyhow::Result<()> {
    let key = app.info().key;
    let prefix = format!("{key}_%");
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE ?")
            .bind(&prefix)
            .fetch_all(pool)
            .await?;

    for (table,) in &tables {
        let has_user_id = sqlx::query_scalar::<_, String>(&format!(
            "SELECT name FROM pragma_table_info('{table}') WHERE name = 'user_id'"
        ))
        .fetch_optional(pool)
        .await?
        .is_some();

        if has_user_id {
            sqlx::query(&format!("DELETE FROM {table} WHERE user_id = ?"))
                .bind(user_id)
                .execute(pool)
                .await?;
        }
    }
    tracing::info!("Cleared all {key} data for user {user_id}");
    Ok(())
}

/// Filter app instances to those enabled by `DEPLOY_APPS`.
pub fn deployed_app_instances(all: Vec<Box<dyn App>>, config: &Config) -> Vec<Box<dyn App>> {
    all.into_iter()
        .filter(|app| config.is_app_deployed(app.info().key))
        .collect()
}
