use axum::{Extension, Router, response::Html, routing::get};

use crate::auth::UserId;
use crate::layout::{NavItem, render_page};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/", get(index))
}

pub fn voice_nav(base: &str, active: &str) -> Vec<NavItem> {
    vec![
        NavItem {
            href: format!("{base}/voice"),
            label: "VoiceToText",
            active: false,
        },
        NavItem {
            href: format!("{base}/voice"),
            label: "Jobs",
            active: active == "jobs",
        },
        NavItem {
            href: format!("{base}/voice/new"),
            label: "New",
            active: active == "new",
        },
        NavItem {
            href: format!("{base}/logout"),
            label: "Log out",
            active: false,
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
fn render_job_row(j: &JobRow, base: &str) -> String {
    let status_class = match j.status.as_str() {
        "done" => "status-done",
        "failed" => "status-failed",
        "processing" => "status-processing",
        _ => "status-pending",
    };
    let id = j.id;
    let detail_link = format!(r##"<a href="{base}/voice/jobs/{id}">View</a>"##);
    let delete_btn = format!(
        r##"<form hx-post="{base}/voice/jobs/{id}/delete"
                hx-target="#voice-jobs-body" hx-swap="innerHTML"
                hx-confirm="Delete this job?">
            <button type="submit" class="btn-icon" title="Delete">&times;</button>
        </form>"##,
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
        completed = j.completed_at.as_deref().unwrap_or("—"),
    )
}

async fn index(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> Html<String> {
    let base = &state.config.base_path;

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
        rows.push_str(&render_job_row(j, base));
    }

    let empty_msg = if jobs.is_empty() {
        r#"<p class="empty-state">No transcription jobs yet. Upload an audio file to get started.</p>"#
    } else {
        ""
    };

    let body = format!(
        r##"<div class="page-header">
            <div class="page-header-row">
                <h1>Voice to Text</h1>
                <a href="{base}/voice/new" class="btn btn-primary">New Transcription</a>
            </div>
            <p>Upload audio files and get text transcriptions</p>
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
                        <th>File</th>
                        <th>Status</th>
                        <th>Model</th>
                        <th>Created</th>
                        <th>Completed</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody id="voice-jobs-body">
                    {rows}
                </tbody>
            </table>
        </div>"##
    );
    Html(render_page(
        "VoiceToText — Jobs",
        &voice_nav(base, "jobs"),
        &body,
        base,
    ))
}
