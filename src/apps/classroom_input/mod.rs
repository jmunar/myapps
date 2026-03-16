mod classrooms;
mod form_types;
mod inputs;
pub mod services;

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

pub fn classroom_nav(base: &str, active: &str) -> Vec<NavItem> {
    vec![
        NavItem {
            href: format!("{base}/classroom"),
            label: "Classroom",
            active: false,
        },
        NavItem {
            href: format!("{base}/classroom"),
            label: "Inputs",
            active: active == "inputs",
        },
        NavItem {
            href: format!("{base}/classroom/classrooms"),
            label: "Classrooms",
            active: active == "classrooms",
        },
        NavItem {
            href: format!("{base}/classroom/form-types"),
            label: "Form Types",
            active: active == "form_types",
        },
        NavItem {
            href: format!("{base}/logout"),
            label: "Log out",
            active: false,
        },
    ]
}
