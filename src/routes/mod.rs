mod auth;
mod launcher;
mod pwa;
mod settings;

use crate::config::Config;
use axum::{Router, middleware};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub llm_lock: Arc<Mutex<()>>,
}

/// Build the application router without binding to a port.
/// Used by both `serve` and integration tests.
pub fn build_router(pool: SqlitePool, config: Config) -> Router {
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        llm_lock: Arc::new(Mutex::new(())),
    };

    // Routes that require authentication
    let mut protected = Router::new()
        .merge(launcher::routes())
        .merge(pwa::push_routes())
        .merge(settings::routes());

    if state.config.is_app_deployed("leanfin") {
        protected = protected.nest("/leanfin", crate::apps::leanfin::router());
    }
    if state.config.is_app_deployed("mindflow") {
        protected = protected.nest("/mindflow", crate::apps::mindflow::router());
    }
    if state.config.is_app_deployed("voice_to_text") {
        protected = protected.nest("/voice", crate::apps::voice_to_text::router());
    }
    if state.config.is_app_deployed("classroom_input") {
        protected = protected.nest("/classroom", crate::apps::classroom_input::router());
    }
    if state.config.llm_enabled() {
        tracing::info!(
            "Command bar enabled (llama server: {})",
            state.config.llama_server_url
        );
        protected = protected.nest("/command", crate::command::routes::routes());
    } else {
        tracing::info!("Command bar disabled (LLAMA_SERVER_URL not set)");
    }

    let protected = protected.layer(middleware::from_fn_with_state(
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

    // Start background voice transcription worker (only if voice_to_text is deployed)
    if config.is_app_deployed("voice_to_text") {
        crate::apps::voice_to_text::services::worker::spawn(pool.clone(), worker_config);
    }

    let app = build_router(pool, config);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
