use axum::{Router, response::Html, routing::get};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../templates/dashboard.html"))
}
