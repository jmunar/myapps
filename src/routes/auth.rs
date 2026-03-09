use axum::{Form, Router, response::{Html, IntoResponse, Redirect}, routing::get};
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use super::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
}

async fn login_page() -> Html<&'static str> {
    Html(include_str!("../templates/login.html"))
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
    match crate::auth::verify_password(&state.pool, &form.username, &form.password).await {
        Ok(user_id) => match crate::auth::create_session(&state.pool, user_id).await {
            Ok(token) => {
                let mut cookie = Cookie::new("session", token);
                cookie.set_http_only(true);
                cookie.set_secure(true);
                cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
                cookie.set_path("/");
                cookies.add(cookie);
                Redirect::to("/").into_response()
            }
            Err(_) => Html("Internal error").into_response(),
        },
        Err(_) => Html("Invalid credentials").into_response(),
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
    Redirect::to("/login")
}
