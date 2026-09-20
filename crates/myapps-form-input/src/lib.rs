mod form_types;
pub mod i18n;
mod inputs;
pub mod ops;
mod row_sets;
pub mod services;

use axum::Router;
use myapps_core::i18n::Lang;
use myapps_core::layout::NavItem;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

/// HTML-escape a string for safe interpolation into HTML element bodies or
/// quoted attribute values. User-controlled strings (labels, names, CSV cells)
/// MUST go through this before reaching `format!` templates.
pub(crate) use myapps_core::components::html_escape;

/// Maximum length of a row-set label (chars). Beyond this we reject the
/// submission rather than truncating, so the user sees their input is wrong.
pub(crate) const MAX_ROW_SET_LABEL_LEN: usize = 200;
/// Maximum total length of a row-set's `rows` blob (bytes after cleaning).
pub(crate) const MAX_ROW_SET_ROWS_BYTES: usize = 64 * 1024;
/// Maximum number of non-empty lines in a row-set.
pub(crate) const MAX_ROW_SET_ROW_COUNT: usize = 5_000;

/// FormInput sub-application router.
/// All routes are relative — the top-level router nests this under `/forms`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(row_sets::routes())
        .merge(form_types::routes())
        .merge(inputs::routes())
}

pub fn forms_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    myapps_core::layout::app_nav(
        base,
        "/forms",
        "Forms",
        active,
        lang,
        &[
            ("", t.inputs, "inputs"),
            ("/row-sets", t.row_sets, "row_sets"),
            ("/form-types", t.form_types, "form_types"),
        ],
    )
}

pub struct FormInputApp;

impl App for FormInputApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "form_input",
            name: "Forms",
            icon: "\u{270E}",
            path: "/forms",
        }
    }

    fn description(&self, lang: myapps_core::i18n::Lang) -> &'static str {
        match lang {
            myapps_core::i18n::Lang::En => "Record structured data with custom forms",
            myapps_core::i18n::Lang::Es => {
                "Registra datos estructurados con formularios personalizados"
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
}
