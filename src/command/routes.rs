use super::{CommandIntent, collect_commands, validate_intent};
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::routes::AppState;
use axum::extract::Extension;
use axum::response::Html;
use axum::{Form, Router, routing::post};
use serde::Deserialize;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/interpret", post(interpret))
        .route("/execute", post(execute))
}

#[derive(Deserialize)]
struct InterpretForm {
    input: String,
}

async fn interpret(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Form(form): Form<InterpretForm>,
) -> Html<String> {
    let t = i18n::t(lang);
    let input = form.input.trim();

    if input.is_empty() {
        return Html(String::new());
    }

    tracing::info!("Command interpret: \"{input}\"");

    // Check LLM availability
    if !state.config.llm_enabled() {
        tracing::warn!("Command bar not configured");
        return Html(format!(
            r#"<div class="command-error">{}</div>"#,
            t.cmd_not_configured
        ));
    }

    // Try to acquire the LLM lock (non-blocking)
    let guard = state.llm_lock.try_lock();
    if guard.is_err() {
        tracing::info!("LLM busy, rejecting request");
        return Html(format!(
            r#"<div class="command-error">{}</div>"#,
            t.cmd_busy
        ));
    }

    let actions = collect_commands(&state.config);
    if actions.is_empty() {
        tracing::warn!("No command actions available (no apps deployed?)");
        return Html(format!(
            r#"<div class="command-error">{}</div>"#,
            t.cmd_no_actions
        ));
    }

    let context = super::collect_command_context(&state.pool, user_id.0, &state.config).await;
    let prompt = super::llm::build_prompt(&actions, input, &context);

    tracing::debug!("Sending prompt to llama server ({} actions)", actions.len());
    let result = super::llm::run_inference(&state.config, &prompt, &actions).await;

    // Release lock (guard drops here)
    drop(guard);

    match result {
        Ok(ref intent) => {
            tracing::info!(
                "LLM response: action={} confidence={:.0}%",
                intent.action,
                intent.confidence * 100.0
            );
            if let Err(err) = validate_intent(intent, &actions) {
                tracing::warn!("Validation failed: {err}");
                return Html(format!(
                    r#"<div class="command-error">{}: {err}</div>"#,
                    t.cmd_error
                ));
            }
            render_confirmation(intent, &actions, &state.config.base_path, lang)
        }
        Err(ref err) => {
            tracing::error!("LLM inference failed: {err}");
            Html(format!(
                r#"<div class="command-error">{}: {err}</div>"#,
                t.cmd_error
            ))
        }
    }
}

/// Render the confirmation card as an HTMX partial.
fn render_confirmation(
    intent: &CommandIntent,
    actions: &[super::CommandAction],
    base_path: &str,
    lang: Lang,
) -> Html<String> {
    let t = i18n::t(lang);

    let action = actions
        .iter()
        .find(|a| format!("{}.{}", a.app, a.name) == intent.action);

    let description = action.map_or(&intent.action as &str, |a| a.description);
    let app_name = action.map_or("", |a| a.app);

    let params_summary: Vec<String> = intent
        .params
        .iter()
        .map(|(k, v)| {
            let display = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("<strong>{k}</strong>: {display}")
        })
        .collect();

    let intent_json = serde_json::to_string(intent).unwrap_or_default();
    // HTML-escape the JSON for safe embedding in a hidden input
    let intent_escaped = intent_json
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    Html(format!(
        r##"<div class="command-confirm">
    <div class="command-confirm-header">
        <span class="command-confirm-app">{app_name}</span>
        <span class="command-confirm-desc">{description}</span>
    </div>
    <div class="command-confirm-params">{params_html}</div>
    <div class="command-confirm-confidence">{confidence_label}: {confidence:.0}%</div>
    <div class="command-confirm-actions">
        <form hx-post="{base_path}/command/execute" hx-target="#command-result" hx-swap="innerHTML">
            <input type="hidden" name="intent" value="{intent_escaped}">
            <button type="submit" class="btn btn-primary">{confirm}</button>
        </form>
        <button type="button" class="btn" onclick="document.getElementById('command-result').innerHTML=''">{cancel}</button>
    </div>
</div>"##,
        params_html = if params_summary.is_empty() {
            format!("<em>{}</em>", t.cmd_no_params)
        } else {
            params_summary.join("<br>")
        },
        confidence_label = t.cmd_confidence,
        confidence = intent.confidence * 100.0,
        confirm = t.cmd_confirm,
        cancel = t.cmd_cancel,
    ))
}

#[derive(Deserialize)]
struct ExecuteForm {
    intent: String,
}

async fn execute(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Form(form): Form<ExecuteForm>,
) -> Html<String> {
    let t = i18n::t(lang);

    let intent: CommandIntent = match serde_json::from_str(&form.intent) {
        Ok(i) => i,
        Err(_) => {
            return Html(format!(
                r#"<div class="command-error">{}</div>"#,
                t.cmd_error
            ));
        }
    };

    // Re-validate
    let actions = collect_commands(&state.config);
    if let Err(err) = validate_intent(&intent, &actions) {
        return Html(format!(
            r#"<div class="command-error">{}: {err}</div>"#,
            t.cmd_error
        ));
    }

    // Dispatch to the appropriate app executor
    let (app, action_name) = match intent.action.split_once('.') {
        Some(pair) => pair,
        None => {
            return Html(format!(
                r#"<div class="command-error">{}</div>"#,
                t.cmd_error
            ));
        }
    };

    let base = &state.config.base_path;
    let result = match app {
        "mindflow" => {
            crate::apps::mindflow::ops::dispatch(
                &state.pool,
                user_id.0,
                action_name,
                &intent.params,
                base,
            )
            .await
        }
        "leanfin" => {
            crate::apps::leanfin::ops::dispatch(
                &state.pool,
                user_id.0,
                action_name,
                &intent.params,
                base,
            )
            .await
        }
        "voice_to_text" => {
            crate::apps::voice_to_text::ops::dispatch(
                &state.pool,
                user_id.0,
                action_name,
                &intent.params,
                base,
            )
            .await
        }
        "classroom_input" => {
            crate::apps::classroom_input::ops::dispatch(
                &state.pool,
                user_id.0,
                action_name,
                &intent.params,
                base,
            )
            .await
        }
        _ => Err(format!("Unknown app: {app}")),
    };

    match result {
        Ok(cmd_result) => {
            if let Some(url) = &cmd_result.redirect {
                return Html(format!(r#"<script>window.location="{url}";</script>"#));
            }
            let class = if cmd_result.success {
                "command-success"
            } else {
                "command-error"
            };
            Html(format!(
                r#"<div class="{class}">{msg}</div>"#,
                msg = cmd_result.message
            ))
        }
        Err(err) => Html(format!(
            r#"<div class="command-error">{}: {err}</div>"#,
            t.cmd_error
        )),
    }
}
