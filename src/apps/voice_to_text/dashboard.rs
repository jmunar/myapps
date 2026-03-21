use axum::{Extension, Router, response::Html, routing::get};

use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::{NavItem, render_page};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn voice_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    vec![
        NavItem {
            href: format!("{base}/voice"),
            label: "VoiceToText".to_string(),
            active: false,
            right: false,
        },
        NavItem {
            href: format!("{base}/voice"),
            label: t.vt_jobs.to_string(),
            active: active == "jobs",
            right: false,
        },
        NavItem {
            href: format!("{base}/voice/new"),
            label: t.vt_new.to_string(),
            active: active == "new",
            right: false,
        },
        NavItem {
            href: format!("{base}/logout"),
            label: t.log_out.to_string(),
            active: false,
            right: true,
        },
    ]
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
    status: String,
    original_filename: String,
    model_used: String,
    created_at: String,
    completed_at: Option<String>,
}

/// Render a single job table row. Shared between the full page and the HTMX partial.
fn render_job_row(j: &JobRow, base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    let status_class = match j.status.as_str() {
        "done" => "status-done",
        "failed" => "status-failed",
        "processing" => "status-processing",
        _ => "status-pending",
    };
    let id = j.id;
    let detail_link = format!(
        r##"<a href="{base}/voice/jobs/{id}">{view}</a>"##,
        view = t.vt_jobs_view
    );
    let delete_btn = format!(
        r##"<form hx-post="{base}/voice/jobs/{id}/delete"
                hx-target="#voice-jobs-body" hx-swap="innerHTML"
                hx-confirm="{confirm}">
            <button type="submit" class="btn-icon">&times;</button>
        </form>"##,
        confirm = t.vt_jobs_delete_confirm,
    );
    format!(
        r##"<tr>
            <td>{filename}</td>
            <td><span class="voice-status {status_class}">{status}</span></td>
            <td>{model}</td>
            <td>{created}</td>
            <td>{completed}</td>
            <td class="voice-actions">{detail_link}{delete_btn}</td>
        </tr>"##,
        filename = j.original_filename,
        status = j.status,
        model = j.model_used,
        created = j.created_at,
        completed = j.completed_at.as_deref().unwrap_or("\u{2014}"),
    )
}

async fn index(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let jobs: Vec<JobRow> = sqlx::query_as(
        "SELECT id, status, original_filename, model_used, created_at, completed_at
         FROM voice_jobs
         WHERE user_id = ?
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut rows = String::new();
    for j in &jobs {
        rows.push_str(&render_job_row(j, base, lang));
    }

    let empty_msg = if jobs.is_empty() {
        format!(r#"<p class="empty-state">{}</p>"#, t.vt_jobs_empty)
    } else {
        String::new()
    };

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>{title}</h1>
                <a href="{base}/voice/new" class="btn btn-primary">{new_btn}</a>
            </div>
            <p>{subtitle}</p>
        </div>
        <div class="card">
            {empty_msg}
            <div hx-get="{base}/voice/jobs/list"
                 hx-trigger="every 5s [document.querySelector('.status-pending,.status-processing')]"
                 hx-target="#voice-jobs-body">
            </div>
            <table class="txn-table">
                <thead>
                    <tr>
                        <th>{col_file}</th>
                        <th>{col_status}</th>
                        <th>{col_model}</th>
                        <th>{col_created}</th>
                        <th>{col_completed}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody id="voice-jobs-body">
                    {rows}
                </tbody>
            </table>
        </div>"##,
        title = t.vt_jobs_title,
        subtitle = t.vt_jobs_subtitle,
        new_btn = t.vt_jobs_new_btn,
        col_file = t.vt_jobs_col_file,
        col_status = t.vt_jobs_col_status,
        col_model = t.vt_jobs_col_model,
        col_created = t.vt_jobs_col_created,
        col_completed = t.vt_jobs_col_completed,
    );
    Html(render_page(
        &format!("VoiceToText \u{2014} {}", t.vt_jobs),
        &voice_nav(base, "jobs", lang),
        &body,
        &state.config,
        lang,
    ))
}
