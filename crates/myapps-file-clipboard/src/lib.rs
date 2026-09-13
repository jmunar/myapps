//! FileClipboard — drop files on one device, pick them up on another.
//!
//! File contents live on the filesystem under `FILE_CLIPBOARD_DIR`, never in
//! SQLite: a 5 GB upload is far past SQLite's blob ceiling, and blobs that size
//! would bloat every copy of the shared `myapps.db`. The database holds
//! metadata, and `services::retention` reconciles the two.

mod files;
pub mod i18n;
pub mod ops;
pub mod services;
pub mod storage;

use axum::Router;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

/// FileClipboard sub-application router.
/// All routes are relative — the top-level router nests this under `/file_clipboard`.
pub fn router() -> Router<AppState> {
    files::routes()
}

pub struct FileClipboardApp;

impl App for FileClipboardApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "file_clipboard",
            name: "FileClipboard",
            description: "Drop files here, pick them up on any device",
            icon: "\u{1F4E5}",
            path: "/file_clipboard",
        }
    }

    fn description(&self, lang: myapps_core::i18n::Lang) -> &'static str {
        match lang {
            myapps_core::i18n::Lang::En => "Drop files here, pick them up on any device",
            myapps_core::i18n::Lang::Es => {
                "Suelta archivos aquí y recógelos en cualquier dispositivo"
            }
        }
    }

    fn css(&self) -> &'static str {
        include_str!("../static/style.css")
    }

    fn migrations(&self) -> sqlx::migrate::Migrator {
        sqlx::migrate!("./migrations")
    }

    fn router(&self) -> Router<AppState> {
        router()
    }

    fn commands(&self) -> Vec<myapps_core::command::CommandAction> {
        ops::commands()
    }

    fn dispatch<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
        action: &'a str,
        params: &'a std::collections::HashMap<String, serde_json::Value>,
        base_path: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<myapps_core::command::CommandResult, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(ops::dispatch(pool, user_id, action, params, base_path))
    }

    fn seed<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>>
    {
        Some(Box::pin(services::seed::run(pool, user_id, self)))
    }

    /// Daily sweep: expire old files, reclaim orphaned bytes.
    fn cron<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        config: &'a myapps_core::config::Config,
    ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>>
    {
        Some(Box::pin(services::retention::sweep(
            pool,
            &config.file_clipboard_dir,
        )))
    }

    /// The sweep also runs in-process, because the system cron job is optional
    /// per environment and a clipboard that never expires fills the disk that
    /// every other app's database sits on.
    fn on_serve(
        &self,
        pool: sqlx::SqlitePool,
        config: std::sync::Arc<myapps_core::config::Config>,
    ) {
        let dir = config.file_clipboard_dir.clone();
        // Surface a misconfigured storage directory at startup rather than on
        // someone's first upload.
        tokio::spawn({
            let dir = dir.clone();
            async move { storage::report_writability(&dir).await }
        });
        services::retention::spawn(pool, dir);
    }
}
