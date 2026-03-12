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
        <span class="brand">LeanFin</span>
        <a href="{base}/" class="active">Transactions</a>
        <a href="{base}/accounts">Accounts</a>
        <a href="{base}/labels">Labels</a>
        <a href="{base}/logout" class="nav-right">Log out</a>
    </nav>
    <main>
        <div class="page-header">
            <h1>Transactions</h1>
            <p>Your recent activity across all accounts</p>
        </div>
        <div class="card">
            <div id="transactions" hx-get="{base}/transactions" hx-trigger="load">
                <div class="loading">Loading transactions</div>
            </div>
        </div>
    </main>
</body>
</html>"#
    ))
}
