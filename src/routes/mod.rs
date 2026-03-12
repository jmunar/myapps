mod auth;
mod launcher;

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

pub async fn serve(pool: SqlitePool, config: Config) -> anyhow::Result<()> {
    let bind_addr = config.bind_addr.clone();
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
    };

    // Routes that require authentication
    let protected = Router::new()
        .merge(launcher::routes())
        .nest("/leanfin", crate::apps::leanfin::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_auth,
        ));

    // Public routes (login/logout)
    let public = auth::routes();

    let app = Router::new()
        .merge(protected)
        .merge(public)
        .nest_service("/static", ServeDir::new("static"))
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on {bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
