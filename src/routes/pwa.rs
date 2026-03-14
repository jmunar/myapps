use axum::http::{Response, header};
use axum::{Router, response::IntoResponse, routing::get};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/manifest.json", get(manifest))
        .route("/sw.js", get(service_worker))
}

async fn manifest(
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let body = serde_json::json!({
        "name": "MyApps",
        "short_name": "MyApps",
        "description": "Your personal platform",
        "start_url": format!("{base}/"),
        "scope": format!("{base}/"),
        "display": "standalone",
        "background_color": "#FAF9F6",
        "theme_color": "#1B2030",
        "icons": [
            {
                "src": format!("{base}/static/icon.svg"),
                "sizes": "any",
                "type": "image/svg+xml",
                "purpose": "any"
            }
        ]
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
}

async fn service_worker(
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let scope = if base.is_empty() {
        "/".to_string()
    } else {
        format!("{base}/")
    };
    let body = format!(
        "const BASE_PATH = \"{base}\";\n{}",
        include_str!("../../static/sw.js"),
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "application/javascript")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("service-worker-allowed", scope)
        .body(body)
        .unwrap()
}
