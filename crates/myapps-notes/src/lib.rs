pub mod i18n;
mod notes;
pub mod ops;
pub mod services;
pub mod sync;

use axum::{Extension, Router};
use myapps_core::i18n::Lang;
use myapps_core::layout::NavItem;
use myapps_core::registry::{App, AppInfo};
use myapps_core::routes::AppState;

pub fn notes_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    let ct = myapps_core::i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/notes"),
            label: "Notes".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/notes"),
            label: t.nav_notes.to_string(),
            active: active == "list",
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

pub struct NotesApp {
    rooms: sync::Rooms,
}

impl Default for NotesApp {
    fn default() -> Self {
        Self::new()
    }
}

impl NotesApp {
    pub fn new() -> Self {
        Self {
            rooms: sync::new_rooms(),
        }
    }
}

impl App for NotesApp {
    fn info(&self) -> AppInfo {
        AppInfo {
            key: "notes",
            name: "Notes",
            description: "Markdown-based note-taking",
            icon: "\u{270F}\u{FE0F}",
            path: "/notes",
        }
    }

    fn description(&self, lang: myapps_core::i18n::Lang) -> &'static str {
        match lang {
            myapps_core::i18n::Lang::En => {
                "Markdown-based note-taking with live preview and voice dictation"
            }
            myapps_core::i18n::Lang::Es => {
                "Toma de notas en Markdown con vista previa y dictado por voz"
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
        Router::new()
            .merge(notes::routes())
            .merge(sync::routes())
            .layer(Extension(self.rooms.clone()))
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

    fn on_serve(
        &self,
        pool: sqlx::SqlitePool,
        _config: std::sync::Arc<myapps_core::config::Config>,
    ) {
        sync::spawn_eviction_task(self.rooms.clone(), pool);
    }
}
