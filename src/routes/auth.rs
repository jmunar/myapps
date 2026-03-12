use axum::{Form, Router, response::{Html, IntoResponse, Redirect}, routing::get};
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
}

async fn login_page(
    state: axum::extract::State<AppState>,
) -> Html<String> {
    let base = &state.config.base_path;
    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LeanFin — Login</title>
    <link rel="stylesheet" href="{base}/static/style.css">
</head>
<body>
    <main class="login-container">
        <h1>LeanFin</h1>
        <form method="POST" action="{base}/login">
            <label for="username">Username</label>
            <input type="text" id="username" name="username" required autofocus>
            <label for="password">Password</label>
            <input type="password" id="password" name="password" required>
            <button type="submit">Log in</button>
        </form>
    </main>
</body>
</html>"#
    ))
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn login_submit(
    cookies: Cookies,
    state: axum::extract::State<AppState>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let base = state.config.base_path.clone();
    match crate::auth::verify_password(&state.pool, &form.username, &form.password).await {
        Ok(user_id) => match crate::auth::create_session(&state.pool, user_id).await {
            Ok(token) => {
                let cookie_path = if base.is_empty() {
                    "/".to_string()
                } else {
                    base.clone()
                };
                let mut cookie = Cookie::new("session", token);
                cookie.set_http_only(true);
                cookie.set_secure(true);
                cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
                cookie.set_path(cookie_path);
                cookies.add(cookie);
                Redirect::to(&format!("{base}/")).into_response()
            }
            Err(_) => Html("Internal error".to_string()).into_response(),
        },
        Err(_) => Html("Invalid credentials".to_string()).into_response(),
    }
}

async fn logout(
    cookies: Cookies,
    state: axum::extract::State<AppState>,
) -> impl IntoResponse {
    if let Some(cookie) = cookies.get("session") {
        let _ = crate::auth::delete_session(&state.pool, cookie.value()).await;
        cookies.remove(Cookie::from("session"));
    }
    Redirect::to(&format!("{}/login", state.config.base_path))
}
