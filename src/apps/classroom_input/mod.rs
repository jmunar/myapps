mod classrooms;
mod form_types;
pub mod i18n;
mod inputs;
pub mod ops;
pub mod services;

use crate::apps::registry::{App, AppInfo};
use crate::i18n::Lang;
use crate::layout::NavItem;
use crate::routes::AppState;
use axum::Router;

/// ClassroomInput sub-application router.
/// All routes are relative — the top-level router nests this under `/classroom`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(classrooms::routes())
        .merge(form_types::routes())
        .merge(inputs::routes())
}

pub fn classroom_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    let ct = crate::i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/classroom"),
            label: "Classroom".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/classroom"),
            label: t.ci_inputs.to_string(),
            active: active == "inputs",
            right: false,
        },
        NavItem {
            href: format!("{base}/classroom/classrooms"),
            label: t.ci_classrooms.to_string(),
            active: active == "classrooms",
            right: false,
        },
        NavItem {
            href: format!("{base}/classroom/form-types"),
            label: t.ci_form_types.to_string(),
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

pub struct ClassroomInputApp;

impl App for ClassroomInputApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "classroom_input",
            name: "ClassroomInput",
            description: "Record marks &amp; notes for classrooms",
            icon: "\u{270E}",
            path: "/classroom",
        }
    }

    fn description(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::En => "Record marks &amp; notes for classrooms",
            crate::i18n::Lang::Es => "Registro de notas y observaciones del aula",
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
        Some(Box::pin(services::seed::run(pool, user_id)))
    }
}
