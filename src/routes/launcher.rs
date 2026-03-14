use axum::{Router, response::Html, routing::get};

use super::AppState;
use crate::layout::{NavItem, render_page};

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

async fn index(
    state: axum::extract::State<AppState>,
) -> Html<String> {
    let base = &state.config.base_path;
    let nav = vec![
        NavItem { href: format!("{base}/logout"), label: "Log out", active: false },
    ];
    let body = format!(
        r#"<div class="page-header">
            <h1>My Apps</h1>
            <p>Choose an application</p>
        </div>
        <div class="launcher-grid">
            <a href="{base}/leanfin" class="launcher-card">
                <div class="launcher-icon">$</div>
                <div class="launcher-info">
                    <h2>LeanFin</h2>
                    <p>Personal expense tracker</p>
                </div>
            </a>
            <a href="{base}/mindflow" class="launcher-card">
                <div class="launcher-icon">&#x1F9E0;</div>
                <div class="launcher-info">
                    <h2>MindFlow</h2>
                    <p>Thought capture &amp; mind map</p>
                </div>
            </a>
        </div>"#
    );
    Html(render_page("MyApps", &nav, &body, base))
}
