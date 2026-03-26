use axum::{
    Form, Router,
    extract::Query,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use super::AppState;
use crate::i18n::{self, Lang};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
}

#[derive(Deserialize, Default)]
struct LangQuery {
    lang: Option<String>,
}

async fn login_page(
    state: axum::extract::State<AppState>,
    cookies: Cookies,
    Query(query): Query<LangQuery>,
) -> Html<String> {
    let base = &state.config.base_path;

    // Determine language: query param > cookie > default
    let lang = if let Some(ref code) = query.lang {
        let l = Lang::from_code(code);
        // Persist choice in cookie
        let mut c = Cookie::new("lang", l.code().to_string());
        c.set_path(if base.is_empty() {
            "/".to_string()
        } else {
            base.clone()
        });
        cookies.add(c);
        l
    } else if let Some(cookie) = cookies.get("lang") {
        Lang::from_code(cookie.value())
    } else {
        Lang::default()
    };

    let t = i18n::t(lang);
    let lang_code = lang.code();

    // Language toggle: show link to the other language
    let (other_code, other_label) = match lang {
        Lang::En => ("es", "Español"),
        Lang::Es => ("en", "English"),
    };

    Html(format!(
        r##"<!DOCTYPE html>
<html lang="{lang_code}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base}/static/core.css">
    <link rel="manifest" href="{base}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base}/static/icon.svg">
    <link rel="apple-touch-icon" href="{base}/static/icon.svg">
</head>
<body class="login-page">
    <script>
    if ("serviceWorker" in navigator) {{
        navigator.serviceWorker.register("{base}/sw.js", {{ scope: "{base}/" }});
    }}
    </script>
    <div class="login-card">
        <div class="login-brand">
            <h1>MyApps</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="card-body">
                <form method="POST" action="{base}/login">
                    <label for="username">{username}</label>
                    <input type="text" id="username" name="username" required autofocus>
                    <label for="password">{password}</label>
                    <input type="password" id="password" name="password" required>
                    <button type="submit">{submit}</button>
                </form>
            </div>
        </div>
        <div style="text-align:center;margin-top:0.75rem">
            <a href="{base}/login?lang={other_code}" style="color:var(--text-secondary);font-size:0.875rem">{other_label}</a>
        </div>
    </div>
</body>
</html>"##,
        title = t.login_title,
        subtitle = t.login_subtitle,
        username = t.login_username,
        password = t.login_password,
        submit = t.login_submit,
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

    // Determine language from cookie for error messages
    let lang = cookies
        .get("lang")
        .map(|c| Lang::from_code(c.value()))
        .unwrap_or_default();
    let t = i18n::t(lang);

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

                // Persist language preference from cookie to DB
                if let Some(lang_cookie) = cookies.get("lang") {
                    let lang = Lang::from_code(lang_cookie.value());
                    let _ = crate::models::user_settings::set_language(&state.pool, user_id, lang)
                        .await;
                }

                Redirect::to(&format!("{base}/")).into_response()
            }
            Err(_) => Html(t.login_error.to_string()).into_response(),
        },
        Err(_) => Html(t.login_invalid.to_string()).into_response(),
    }
}

async fn logout(cookies: Cookies, state: axum::extract::State<AppState>) -> impl IntoResponse {
    if let Some(cookie) = cookies.get("session") {
        let _ = crate::auth::delete_session(&state.pool, cookie.value()).await;
        cookies.remove(Cookie::from("session"));
    }
    Redirect::to(&format!("{}/login", state.config.base_path))
}
