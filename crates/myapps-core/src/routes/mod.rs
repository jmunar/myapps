mod auth;
mod invite;
mod launcher;
mod pwa;
mod settings;

use crate::config::Config;
use crate::registry::App;
use axum::http::header;
use axum::response::Response;
use axum::{Router, middleware, routing::get};
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
    pub apps: Arc<Vec<Box<dyn App>>>,
}

/// Build the application router without binding to a port.
/// Used by both `serve` and integration tests.
pub fn build_router(pool: SqlitePool, config: Config, apps: Vec<Box<dyn App>>) -> Router {
    tracing::info!("Static assets version: {}", config.static_version);

    // Routes that require authentication
    let mut protected = Router::new()
        .merge(launcher::routes())
        .merge(pwa::push_routes())
        .merge(settings::routes());

    for app in &apps {
        let info = app.info();
        protected = protected.nest(info.path, app.router());
    }
    if config.llm_enabled() {
        tracing::info!(
            "Command bar enabled (llama server: {})",
            config.llama_server_url
        );
        protected = protected.nest("/command", crate::command::routes::routes());
    } else {
        tracing::info!("Command bar disabled (LLAMA_SERVER_URL not set)");
    }

    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        llm_lock: Arc::new(Mutex::new(())),
        apps: Arc::new(apps),
    };

    let protected = protected.layer(middleware::from_fn_with_state(
        state.clone(),
        crate::auth::require_auth,
    ));

    // Public routes (login/logout, invite registration)
    let public = auth::routes().merge(invite::routes());

    Router::new()
        .merge(protected)
        .merge(public)
        .merge(pwa::routes())
        .route("/static/apps.css", get(apps_css))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CookieManagerLayer::new())
        .with_state(state)
}

/// Serve concatenated app CSS with long-term caching (busted via ?v= query param).
async fn apps_css(state: axum::extract::State<AppState>) -> Response {
    let css: String = state
        .apps
        .iter()
        .map(|a| a.css())
        .collect::<Vec<_>>()
        .join("\n");
    Response::builder()
        .header(header::CONTENT_TYPE, "text/css")
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(css.into())
        .unwrap()
}

pub async fn serve(
    pool: SqlitePool,
    config: Config,
    apps: Vec<Box<dyn App>>,
) -> anyhow::Result<()> {
    let bind_addr = config.bind_addr.clone();
    let worker_config = Arc::new(config.clone());

    for app in &apps {
        app.on_serve(pool.clone(), worker_config.clone());
    }

    let app = build_router(pool, config, apps);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
