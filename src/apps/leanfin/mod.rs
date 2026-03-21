mod accounts;
mod balance_evolution;
mod dashboard;
mod expenses;
mod labels;
pub mod ops;
pub mod services;
pub mod settings;
mod sync_handler;
mod transactions;

use crate::routes::AppState;
use axum::Router;

/// LeanFin sub-application router.
/// All routes are relative — the top-level router nests this under `/leanfin`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(dashboard::routes())
        .merge(transactions::routes())
        .merge(accounts::routes())
        .merge(labels::routes())
        .merge(sync_handler::routes())
        .merge(balance_evolution::routes())
        .merge(expenses::routes())
        .merge(settings::routes())
}
