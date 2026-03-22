use axum::{
    Form, Router,
    extract::{Path, Query},
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use super::AppState;
use crate::auth::InviteError;
use crate::i18n::{self, Lang};

pub fn routes() -> Router<AppState> {
    Router::new().route("/invite/{token}", get(invite_page).post(invite_submit))
}

#[derive(Deserialize, Default)]
struct LangQuery {
    lang: Option<String>,
}

async fn invite_page(
    state: axum::extract::State<AppState>,
    cookies: Cookies,
    Path(token): Path<String>,
    Query(query): Query<LangQuery>,
) -> Html<String> {
    let base = &state.config.base_path;

    let lang = resolve_lang(&cookies, &query.lang, base);
    let t = i18n::t(lang);

    match crate::auth::validate_invite(&state.pool, &token).await {
        Ok(()) => Html(render_form(base, lang, t, &token, None)),
        Err(e) => Html(render_error(base, lang, t, invite_error_message(t, &e))),
    }
}

#[derive(Deserialize)]
struct RegisterForm {
    username: String,
    password: String,
    confirm_password: String,
}

async fn invite_submit(
    state: axum::extract::State<AppState>,
    cookies: Cookies,
    Path(token): Path<String>,
    Form(form): Form<RegisterForm>,
) -> impl IntoResponse {
    let base = state.config.base_path.clone();

    let lang = cookies
        .get("lang")
        .map(|c| Lang::from_code(c.value()))
        .unwrap_or_default();
    let t = i18n::t(lang);

    // Re-validate invite
    if let Err(e) = crate::auth::validate_invite(&state.pool, &token).await {
        return Html(render_error(&base, lang, t, invite_error_message(t, &e))).into_response();
    }

    // Validate passwords match
    if form.password != form.confirm_password {
        return Html(render_form(
            &base,
            lang,
            t,
            &token,
            Some(t.invite_passwords_mismatch),
        ))
        .into_response();
    }

    // Create user
    let user_id = match crate::auth::create_user(&state.pool, &form.username, &form.password).await
    {
        Ok(id) => id,
        Err(_) => {
            return Html(render_form(
                &base,
                lang,
                t,
                &token,
                Some(t.invite_username_taken),
            ))
            .into_response();
        }
    };

    // Mark invite as used
    let _ = crate::auth::mark_invite_used(&state.pool, &token).await;

    // Auto-seed deployed apps for new user if configured
    if state.config.seed {
        if state.config.is_app_deployed("leanfin")
            && let Err(e) = crate::apps::leanfin::services::seed::run(&state.pool, user_id).await
        {
            tracing::error!("Failed to seed leanfin for user {user_id}: {e}");
        }
        if state.config.is_app_deployed("mindflow")
            && let Err(e) = crate::apps::mindflow::services::seed::run(&state.pool, user_id).await
        {
            tracing::error!("Failed to seed mindflow for user {user_id}: {e}");
        }
        if state.config.is_app_deployed("classroom_input")
            && let Err(e) =
                crate::apps::classroom_input::services::seed::run(&state.pool, user_id).await
        {
            tracing::error!("Failed to seed classroom for user {user_id}: {e}");
        }
    }

    // Create session and log them in
    match crate::auth::create_session(&state.pool, user_id).await {
        Ok(session_token) => {
            let cookie_path = if base.is_empty() {
                "/".to_string()
            } else {
                base.clone()
            };
            let mut cookie = Cookie::new("session", session_token);
            cookie.set_http_only(true);
            cookie.set_secure(true);
            cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
            cookie.set_path(cookie_path);
            cookies.add(cookie);

            if let Some(lang_cookie) = cookies.get("lang") {
                let lang = Lang::from_code(lang_cookie.value());
                let _ =
                    crate::models::user_settings::set_language(&state.pool, user_id, lang).await;
            }

            Redirect::to(&format!("{base}/")).into_response()
        }
        Err(_) => Html(render_error(&base, lang, t, t.invite_invalid)).into_response(),
    }
}

fn resolve_lang(cookies: &Cookies, query_lang: &Option<String>, base: &str) -> Lang {
    if let Some(code) = query_lang {
        let l = Lang::from_code(code);
        let mut c = Cookie::new("lang", l.code().to_string());
        c.set_path(if base.is_empty() {
            "/".to_string()
        } else {
            base.to_string()
        });
        cookies.add(c);
        l
    } else if let Some(cookie) = cookies.get("lang") {
        Lang::from_code(cookie.value())
    } else {
        Lang::default()
    }
}

fn invite_error_message<'a>(t: &'a i18n::Translations, err: &InviteError) -> &'a str {
    match err {
        InviteError::NotFound => t.invite_invalid,
        InviteError::Expired => t.invite_expired,
        InviteError::AlreadyUsed => t.invite_used,
    }
}

fn render_form(
    base: &str,
    lang: Lang,
    t: &i18n::Translations,
    token: &str,
    error: Option<&str>,
) -> String {
    let lang_code = lang.code();
    let (other_code, other_label) = match lang {
        Lang::En => ("es", "Español"),
        Lang::Es => ("en", "English"),
    };

    let error_html = error
        .map(|msg| {
            format!(
                r#"<div style="color:var(--danger);margin-bottom:1rem;text-align:center">{msg}</div>"#
            )
        })
        .unwrap_or_default();

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang_code}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base}/static/style.css">
    <link rel="manifest" href="{base}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base}/static/icon.svg">
    <link rel="apple-touch-icon" href="{base}/static/icon.svg">
</head>
<body class="login-page">
    <div class="login-card">
        <div class="login-brand">
            <h1>MyApps</h1>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            <div class="card-body">
                {error_html}
                <form method="POST" action="{base}/invite/{token}">
                    <label for="username">{username}</label>
                    <input type="text" id="username" name="username" required autofocus>
                    <label for="password">{password}</label>
                    <input type="password" id="password" name="password" required>
                    <label for="confirm_password">{confirm_password}</label>
                    <input type="password" id="confirm_password" name="confirm_password" required>
                    <button type="submit">{submit}</button>
                </form>
            </div>
        </div>
        <div style="text-align:center;margin-top:0.75rem">
            <a href="{base}/invite/{token}?lang={other_code}" style="color:var(--text-secondary);font-size:0.875rem">{other_label}</a>
        </div>
    </div>
</body>
</html>"##,
        title = t.invite_title,
        subtitle = t.invite_subtitle,
        username = t.invite_username,
        password = t.invite_password,
        confirm_password = t.invite_confirm_password,
        submit = t.invite_submit,
    )
}

fn render_error(base: &str, lang: Lang, t: &i18n::Translations, message: &str) -> String {
    let lang_code = lang.code();
    let (other_code, other_label) = match lang {
        Lang::En => ("es", "Español"),
        Lang::Es => ("en", "English"),
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang_code}">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="theme-color" content="#1B2030">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>{title}</title>
    <link rel="stylesheet" href="{base}/static/style.css">
    <link rel="manifest" href="{base}/manifest.json">
    <link rel="icon" type="image/svg+xml" href="{base}/static/icon.svg">
    <link rel="apple-touch-icon" href="{base}/static/icon.svg">
</head>
<body class="login-page">
    <div class="login-card">
        <div class="login-brand">
            <h1>MyApps</h1>
            <p>{message}</p>
        </div>
        <div class="card">
            <div class="card-body" style="text-align:center">
                <a href="{base}/login">{login}</a>
            </div>
        </div>
        <div style="text-align:center;margin-top:0.75rem">
            <a href="{base}/login?lang={other_code}" style="color:var(--text-secondary);font-size:0.875rem">{other_label}</a>
        </div>
    </div>
</body>
</html>"##,
        title = t.invite_title,
        login = t.login_submit,
    )
}
