mod dashboard;
pub mod i18n;
mod jobs;
pub mod ops;
pub mod services;

use crate::apps::registry::{App, AppInfo};
use crate::routes::AppState;
use axum::Router;

/// VoiceToText sub-application router.
/// All routes are relative — the top-level router nests this under `/voice`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::routes())
        .merge(jobs::routes())
}

pub struct VoiceToTextApp;

impl App for VoiceToTextApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "voice_to_text",
            name: "VoiceToText",
            description: "Audio transcription with Whisper",
            icon: "\u{1F3A4}",
            path: "/voice",
        }
    }

    fn description(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::En => "Audio transcription with Whisper",
            crate::i18n::Lang::Es => "Transcripción de audio con Whisper",
        }
    }

    fn migrations(&self) -> sqlx::migrate::Migrator {
        sqlx::migrate!("src/apps/voice_to_text/migrations")
    }

    fn router(&self) -> Router<AppState> {
        router()
    }

    fn commands(&self) -> Vec<crate::command::CommandAction> {
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
            dyn std::future::Future<Output = Result<crate::command::CommandResult, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(ops::dispatch(pool, user_id, action, params, base_path))
    }

    fn on_serve(&self, pool: sqlx::SqlitePool, config: std::sync::Arc<crate::config::Config>) {
        services::worker::spawn(pool, config);
    }
}
