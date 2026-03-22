mod accounts;
mod balance_evolution;
mod dashboard;
mod expenses;
pub mod i18n;
mod labels;
pub mod ops;
pub mod services;
pub mod settings;
mod sync_handler;
mod transactions;

use crate::apps::registry::{App, AppInfo};
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

pub struct LeanFinApp;

impl App for LeanFinApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "leanfin",
            name: "LeanFin",
            description: "Personal expense tracker",
            icon: "$",
            path: "/leanfin",
        }
    }

    fn description(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::En => "Personal expense tracker",
            crate::i18n::Lang::Es => "Gestor de gastos personales",
        }
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

    fn seed<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>>
    {
        Some(Box::pin(services::seed::run(pool, user_id)))
    }
}
