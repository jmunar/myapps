mod dashboard;
mod jobs;
pub mod ops;
pub mod services;

use crate::routes::AppState;
use axum::Router;

/// VoiceToText sub-application router.
/// All routes are relative — the top-level router nests this under `/voice`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::routes())
        .merge(jobs::routes())
}
