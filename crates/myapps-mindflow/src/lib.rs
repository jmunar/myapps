mod actions;
mod categories;
pub mod i18n;
mod inbox;
mod mind_map;
pub mod ops;
pub mod services;
mod thoughts;

use axum::Router;
use myapps_core::i18n::Lang;
use myapps_core::layout::NavItem;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

/// MindFlow sub-application router.
/// All routes are relative — the top-level router nests this under `/mindflow`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(mind_map::routes())
        .merge(categories::routes())
        .merge(thoughts::routes())
        .merge(inbox::routes())
        .merge(actions::routes())
}

pub fn mindflow_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    myapps_core::layout::app_nav(
        base,
        "/mindflow",
        "MindFlow",
        active,
        lang,
        &[
            ("", t.mind_map, "map"),
            ("/inbox", t.inbox, "inbox"),
            ("/actions", t.actions, "actions"),
            ("/categories", t.categories, "categories"),
        ],
    )
}

pub struct MindFlowApp;

impl App for MindFlowApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "mindflow",
            name: "MindFlow",
            icon: "\u{1F9E0}",
            path: "/mindflow",
        }
    }

    fn description(&self, lang: myapps_core::i18n::Lang) -> &'static str {
        match lang {
            myapps_core::i18n::Lang::En => "Thought capture &amp; mind map",
            myapps_core::i18n::Lang::Es => "Captura de ideas y mapa mental",
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
    ) -> myapps_core::registry::BoxFuture<'a, Result<myapps_core::command::CommandResult, String>>
    {
        Box::pin(ops::dispatch(pool, user_id, action, params, base_path))
    }

    fn seed<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> Option<myapps_core::registry::BoxFuture<'a, anyhow::Result<()>>> {
        Some(Box::pin(services::seed::run(pool, user_id, self)))
    }
}
