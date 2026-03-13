mod accounts;
mod dashboard;
mod labels;
pub mod services;
mod sync_handler;
mod transactions;

use axum::Router;
use crate::routes::AppState;

/// LeanFin sub-application router.
/// All routes are relative — the top-level router nests this under `/leanfin`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::routes())
        .merge(transactions::routes())
        .merge(accounts::routes())
        .merge(labels::routes())
        .merge(sync_handler::routes())
}
