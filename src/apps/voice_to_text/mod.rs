mod dashboard;
mod jobs;
pub mod services;

use axum::Router;
use crate::routes::AppState;

/// VoiceToText sub-application router.
/// All routes are relative — the top-level router nests this under `/voice`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::routes())
        .merge(jobs::routes())
}
