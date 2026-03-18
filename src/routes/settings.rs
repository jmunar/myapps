use axum::{Extension, Form, Router, response::Redirect, routing::post};
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use super::AppState;
use crate::auth::UserId;
use crate::i18n::Lang;
use crate::models::user_settings;

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings/language", post(set_language))
}

#[derive(Deserialize)]
struct LangForm {
    language: String,
    redirect: Option<String>,
}

async fn set_language(
    cookies: Cookies,
    state: axum::extract::State<AppState>,
    Extension(UserId(user_id)): Extension<UserId>,
    Form(form): Form<LangForm>,
) -> Redirect {
    let base = &state.config.base_path;
    let lang = Lang::from_code(&form.language);

    let _ = user_settings::set_language(&state.pool, user_id, lang).await;

    // Also update the cookie so it persists for public pages
    let cookie_path = if base.is_empty() {
        "/".to_string()
    } else {
        base.clone()
    };
    let mut c = Cookie::new("lang", lang.code().to_string());
    c.set_path(cookie_path);
    cookies.add(c);

    let redirect_to = form
        .redirect
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| format!("{base}/"));
    Redirect::to(&redirect_to)
}
