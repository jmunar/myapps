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
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// Per-app scoped pools (keyed by app key). Each pool's connections can only
    /// access tables whose name starts with `<app_key>_`.
    pub app_pools: Arc<HashMap<&'static str, SqlitePool>>,
    pub config: Arc<Config>,
    pub llm_lock: Arc<Mutex<()>>,
    pub apps: Arc<Vec<Box<dyn App>>>,
}

/// Build the application router without binding to a port.
/// Used by both `serve` and integration tests.
pub fn build_router(
    pool: SqlitePool,
    app_pools: HashMap<&'static str, SqlitePool>,
    config: Config,
    apps: Vec<Box<dyn App>>,
) -> Router {
    tracing::info!("Static assets version: {}", config.static_version);

    let state = AppState {
        pool: pool.clone(),
        app_pools: Arc::new(app_pools),
        config: Arc::new(config),
        llm_lock: Arc::new(Mutex::new(())),
        apps: Arc::new(apps),
    };

    // Routes that require authentication
    let mut protected = Router::new()
        .merge(launcher::routes())
        .merge(pwa::push_routes())
        .merge(settings::routes());

    // Each app gets its own AppState where `.pool` is the scoped pool, so app
    // handlers transparently use only their allowed tables.
    for app in state.apps.iter() {
        let info = app.info();
        let app_state = AppState {
            pool: state
                .app_pools
                .get(info.key)
                .cloned()
                .unwrap_or_else(|| state.pool.clone()),
            ..state.clone()
        };
        let resolved = app.router().with_state(app_state);
        protected = protected.nest_service(info.path, resolved);
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
    app_pools: HashMap<&'static str, SqlitePool>,
    config: Config,
    apps: Vec<Box<dyn App>>,
) -> anyhow::Result<()> {
    let bind_addr = config.bind_addr.clone();
    let worker_config = Arc::new(config.clone());

    for app in &apps {
        let key = app.info().key;
        let scoped = app_pools.get(key).cloned().unwrap_or_else(|| pool.clone());
        app.on_serve(scoped, worker_config.clone());
    }

    let app = build_router(pool, app_pools, config, apps);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
