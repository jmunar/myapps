pub mod bundle;
pub mod dataset;
pub mod diagram;
pub mod i18n;
pub mod ops;
mod pages;
pub mod selector;
pub mod services;

use axum::Router;
use myapps_core::i18n::Lang;
use myapps_core::layout::NavItem;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

/// Problem text, solutions, subjects and topics all come from third-party
/// datasets and MUST go through this before reaching a `format!` template.
pub(crate) use myapps_core::components::html_escape;

pub fn challenges_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    myapps_core::layout::app_nav(
        base,
        "/challenges",
        "Challenges",
        active,
        lang,
        &[
            ("", t.nav_practice, "practice"),
            ("/stats", t.nav_stats, "stats"),
        ],
    )
}

pub struct ChallengesApp;

impl App for ChallengesApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "challenges",
            name: "Challenges",
            icon: "\u{1F3AF}",
            path: "/challenges",
        }
    }

    fn description(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::En => "Daily maths and physics problems, graded to your level",
            Lang::Es => "Problemas diarios de matemáticas y física, adaptados a tu nivel",
        }
    }

    fn css(&self) -> &'static str {
        include_str!("../static/style.css")
    }

    fn migrations(&self) -> sqlx::migrate::Migrator {
        sqlx::migrate!("./migrations")
    }

    fn router(&self) -> Router<AppState> {
        pages::routes()
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

    fn command_context<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> myapps_core::registry::BoxFuture<'a, std::collections::HashMap<String, String>> {
        Box::pin(ops::command_context(pool, user_id))
    }

    fn seed<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> Option<myapps_core::registry::BoxFuture<'a, anyhow::Result<()>>> {
        Some(Box::pin(services::seed::run(pool, user_id, self)))
    }

    fn import<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        _config: &'a myapps_core::config::Config,
        what: &'a str,
    ) -> Option<myapps_core::registry::BoxFuture<'a, anyhow::Result<()>>> {
        Some(Box::pin(services::import::run(pool, what)))
    }
}
