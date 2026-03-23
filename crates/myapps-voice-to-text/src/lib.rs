mod dashboard;
pub mod i18n;
mod jobs;
pub mod ops;
pub mod services;

use axum::Router;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

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

    fn description(&self, lang: myapps_core::i18n::Lang) -> &'static str {
        match lang {
            myapps_core::i18n::Lang::En => "Audio transcription with Whisper",
            myapps_core::i18n::Lang::Es => "Transcripción de audio con Whisper",
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

    fn on_serve(
        &self,
        pool: sqlx::SqlitePool,
        config: std::sync::Arc<myapps_core::config::Config>,
    ) {
        services::worker::spawn(pool, config);
    }
}
