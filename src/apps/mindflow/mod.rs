mod actions;
mod categories;
mod inbox;
mod mind_map;
pub mod ops;
pub mod services;
mod thoughts;

use crate::i18n::{self, Lang};
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

pub fn mindflow_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/mindflow"),
            label: "MindFlow".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/mindflow"),
            label: t.mf_mind_map.to_string(),
            active: active == "map",
            right: false,
        },
        NavItem {
            href: format!("{base}/mindflow/inbox"),
            label: t.mf_inbox.to_string(),
            active: active == "inbox",
            right: false,
        },
        NavItem {
            href: format!("{base}/mindflow/actions"),
            label: t.mf_actions.to_string(),
            active: active == "actions",
            right: false,
        },
        NavItem {
            href: format!("{base}/mindflow/categories"),
            label: t.mf_categories.to_string(),
            active: active == "categories",
            right: false,
        },
        NavItem {
            href: format!("{base}/logout"),
            label: t.log_out.to_string(),
            active: false,
            right: true,
        },
    ]
}
