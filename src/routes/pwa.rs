use axum::http::{Response, StatusCode, header};
use axum::{Json, Router, response::IntoResponse, routing::get, routing::post};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/manifest.json", get(manifest))
        .route("/sw.js", get(service_worker))
}

pub fn push_routes() -> Router<AppState> {
    Router::new()
        .route("/push/vapid-key", get(vapid_key))
        .route("/push/subscribe", post(subscribe))
        .route("/push/unsubscribe", post(unsubscribe))
}

async fn manifest(state: axum::extract::State<AppState>) -> impl IntoResponse {
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

async fn service_worker(state: axum::extract::State<AppState>) -> impl IntoResponse {
    let base = &state.config.base_path;
    let sv = &state.config.static_version;
    let scope = if base.is_empty() {
        "/".to_string()
    } else {
        format!("{base}/")
    };
    let body = format!(
        "const BASE_PATH = \"{base}\";\nconst STATIC_VERSION = \"{sv}\";\n{}",
        include_str!("../../static/sw.js"),
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "application/javascript")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("service-worker-allowed", scope)
        .body(body)
        .unwrap()
}

async fn vapid_key(state: axum::extract::State<AppState>) -> impl IntoResponse {
    match &state.config.vapid_public_key {
        Some(key) => (StatusCode::OK, key.clone()),
        None => (StatusCode::NOT_FOUND, "VAPID not configured".to_string()),
    }
}

#[derive(serde::Deserialize)]
struct PushSubscription {
    endpoint: String,
    p256dh: String,
    auth: String,
}

async fn subscribe(
    state: axum::extract::State<AppState>,
    axum::Extension(user_id): axum::Extension<crate::auth::UserId>,
    Json(sub): Json<PushSubscription>,
) -> impl IntoResponse {
    let result = sqlx::query(
        r#"INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(endpoint) DO UPDATE SET p256dh = excluded.p256dh, auth = excluded.auth"#,
    )
    .bind(user_id.0)
    .bind(&sub.endpoint)
    .bind(&sub.p256dh)
    .bind(&sub.auth)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!("Failed to save push subscription: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

#[derive(serde::Deserialize)]
struct Unsubscribe {
    endpoint: String,
}

async fn unsubscribe(
    state: axum::extract::State<AppState>,
    Json(body): Json<Unsubscribe>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
        .bind(&body.endpoint)
        .execute(&state.pool)
        .await;
    StatusCode::OK
}
