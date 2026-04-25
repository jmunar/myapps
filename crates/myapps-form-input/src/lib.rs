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
    let ct = myapps_core::i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/forms"),
            label: "Forms".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/forms"),
            label: t.inputs.to_string(),
            active: active == "inputs",
            right: false,
        },
        NavItem {
            href: format!("{base}/forms/row-sets"),
            label: t.row_sets.to_string(),
            active: active == "row_sets",
            right: false,
        },
        NavItem {
            href: format!("{base}/forms/form-types"),
            label: t.form_types.to_string(),
            active: active == "form_types",
            right: false,
        },
        NavItem {
            href: format!("{base}/logout"),
            label: ct.log_out.to_string(),
            active: false,
            right: true,
        },
    ]
}

pub struct FormInputApp;

impl App for FormInputApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "form_input",
            name: "Forms",
            description: "Record structured data with custom forms",
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
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<myapps_core::command::CommandResult, String>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(ops::dispatch(pool, user_id, action, params, base_path))
    }

    fn command_context<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::collections::HashMap<String, String>> + Send + 'a,
        >,
    > {
        Box::pin(ops::command_context(pool, user_id))
    }

    fn seed<'a>(
        &'a self,
        pool: &'a sqlx::SqlitePool,
        user_id: i64,
    ) -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>>
    {
        Some(Box::pin(services::seed::run(pool, user_id, self)))
    }
}
