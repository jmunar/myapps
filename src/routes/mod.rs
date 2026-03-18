mod auth;
mod launcher;
mod pwa;
mod settings;

use crate::config::Config;
use axum::{Router, middleware};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
}

/// Build the application router without binding to a port.
/// Used by both `serve` and integration tests.
pub fn build_router(pool: SqlitePool, config: Config) -> Router {
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
    };

    // Routes that require authentication
    let protected = Router::new()
        .merge(launcher::routes())
        .merge(pwa::push_routes())
        .nest("/leanfin", crate::apps::leanfin::router())
        .nest("/mindflow", crate::apps::mindflow::router())
        .nest("/voice", crate::apps::voice_to_text::router())
        .nest("/classroom", crate::apps::classroom_input::router())
        .merge(settings::routes())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_auth,
        ));

    // Public routes (login/logout)
    let public = auth::routes();

    Router::new()
        .merge(protected)
        .merge(public)
        .merge(pwa::routes())
        .nest_service("/static", ServeDir::new("static"))
        .layer(CookieManagerLayer::new())
        .with_state(state)
}

pub async fn serve(pool: SqlitePool, config: Config) -> anyhow::Result<()> {
    let bind_addr = config.bind_addr.clone();
    let worker_config = Arc::new(config.clone());

    // Start background voice transcription worker
    crate::apps::voice_to_text::services::worker::spawn(pool.clone(), worker_config);

    let app = build_router(pool, config);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
