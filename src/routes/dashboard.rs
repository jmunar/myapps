use axum::{Router, response::Html, routing::get};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

async fn index(
    state: axum::extract::State<AppState>,
) -> Html<String> {
    let base = &state.config.base_path;
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LeanFin</title>
    <link rel="stylesheet" href="{base}/static/style.css">
    <script src="{base}/static/htmx.min.js"></script>
</head>
<body>
    <nav>
        <strong>LeanFin</strong>
        <a href="{base}/logout">Log out</a>
    </nav>
    <main>
        <h1>Dashboard</h1>
        <section>
            <h2>Recent transactions</h2>
            <div id="transactions" hx-get="{base}/transactions" hx-trigger="load">
                Loading...
            </div>
        </section>
    </main>
</body>
</html>"#
    ))
}
