mod actions;
mod categories;
mod inbox;
mod mind_map;
pub mod services;
mod thoughts;

use crate::layout::NavItem;
use crate::routes::AppState;
use axum::Router;

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

pub fn mindflow_nav(base: &str, active: &str) -> Vec<NavItem> {
    vec![
        NavItem {
            href: format!("{base}/mindflow"),
            label: "MindFlow",
            active: false,
        },
        NavItem {
            href: format!("{base}/mindflow"),
            label: "Mind Map",
            active: active == "map",
        },
        NavItem {
            href: format!("{base}/mindflow/inbox"),
            label: "Inbox",
            active: active == "inbox",
        },
        NavItem {
            href: format!("{base}/mindflow/actions"),
            label: "Actions",
            active: active == "actions",
        },
        NavItem {
            href: format!("{base}/mindflow/categories"),
            label: "Categories",
            active: active == "categories",
        },
        NavItem {
            href: format!("{base}/logout"),
            label: "Log out",
            active: false,
        },
    ]
}
