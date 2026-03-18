mod classrooms;
mod form_types;
mod inputs;
pub mod services;

use crate::i18n::{self, Lang};
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
            label: t.log_out.to_string(),
            active: false,
            right: true,
        },
    ]
}
