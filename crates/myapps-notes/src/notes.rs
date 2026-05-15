use axum::{
    Extension, Form, Router,
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;

use super::notes_nav;
use myapps_core::auth::UserId;
use myapps_core::i18n::Lang;
use myapps_core::layout::render_page;
use myapps_core::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/new", post(create))
        .route("/{id}/edit", get(edit))
        .route("/{id}/denorm", post(denorm))
        .route("/{id}/delete", post(delete))
        .route("/{id}/toggle-pin", post(toggle_pin))
        .route("/{id}/dictate", post(dictate))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct NoteRow {
    id: i64,
    client_uuid: String,
    title: String,
    body: String,
    pinned: i64,
    created_at: String,
    updated_at: String,
}

async fn list(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let notes: Vec<NoteRow> = sqlx::query_as(
        "SELECT id, client_uuid, title, body, pinned, created_at, updated_at FROM notes_notes WHERE user_id = ? ORDER BY pinned DESC, updated_at DESC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("DB query failed: {e:#}");
        Default::default()
    });

    let mut items = String::new();
    for n in &notes {
        let title_display = if n.title.is_empty() {
            t.untitled
        } else {
            &n.title
        };
        // Extract first non-empty, non-heading line as preview
        let preview: String = n
            .body
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .take(1)
            .map(|l| {
                let s = l.trim();
                if s.len() > 120 {
                    format!("{}…", &s[..120])
                } else {
                    s.to_string()
                }
            })
            .collect();

        let pin_badge = if n.pinned != 0 {
            format!(r#"<span class="notes-pin-badge">{}</span>"#, t.pinned)
        } else {
            String::new()
        };

        let date = &n.updated_at[..10]; // YYYY-MM-DD

        items.push_str(&format!(
            r##"<a href="{base}/notes/{id}/edit" class="notes-card">
                <div class="notes-card-header">
                    <span class="notes-card-title">{title}{pin_badge}</span>
                    <span class="notes-card-date">{date}</span>
                </div>
                <div class="notes-card-preview">{preview}</div>
            </a>"##,
            id = n.id,
            title = html_escape(title_display),
            preview = html_escape(&preview),
        ));
    }

    if items.is_empty() {
        items = format!(r#"<div class="empty-state"><p>{}</p></div>"#, t.empty);
    }

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="notes-toolbar">
            <form method="POST" action="{base}/notes/new">
                <button type="submit" class="btn btn-primary">{new_note}</button>
            </form>
        </div>

        <div class="notes-grid">{items}</div>"##,
        title = t.title,
        subtitle = t.subtitle,
        new_note = t.new_note,
    );

    Html(render_page(
        &format!("Notes — {}", t.title),
        &notes_nav(base, "list", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn create(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let id = super::ops::create_note(&state.pool, user_id.0, "", "")
        .await
        .unwrap_or(0);
    Redirect::to(&format!("{base}/notes/{id}/edit"))
}

async fn edit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    let t = super::i18n::t(lang);

    let note: Option<NoteRow> = sqlx::query_as(
        "SELECT id, client_uuid, title, body, pinned, created_at, updated_at FROM notes_notes WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    let Some(note) = note else {
        return Html(render_page(
            "Notes",
            &notes_nav(base, "edit", lang),
            r#"<div class="empty-state"><p>Note not found.</p></div>"#,
            &state.config,
            lang,
        ));
    };

    let pin_label = if note.pinned != 0 { t.unpin } else { t.pin };
    let sv = &state.config.static_version;
    let whisper_available = state.config.whisper_available();

    let dictate_btn = if whisper_available {
        format!(
            r##"<button type="button" id="notes-dictate-btn" class="btn btn-secondary"
                    data-url="{base}/notes/{id}/dictate"
                    data-recording-label="{dictating}"
                    data-transcribing-label="{transcribing}">{dictate}</button>"##,
            dictate = t.dictate,
            dictating = t.dictating,
            transcribing = t.transcribing,
        )
    } else {
        String::new()
    };

    let body = format!(
        r##"<div class="notes-editor-container">
            <div class="notes-editor-toolbar">
                <input type="text" id="notes-title-input" placeholder="{untitled}"
                       class="notes-title-input" autocomplete="off">
                <div class="notes-editor-actions">
                    {dictate_btn}
                    <form method="POST" action="{base}/notes/{id}/toggle-pin" class="notes-pin-form">
                        <button type="submit" class="btn btn-secondary">{pin_label}</button>
                    </form>
                    <a href="{base}/notes" class="btn btn-secondary">{back}</a>
                </div>
            </div>
            <div class="notes-editor-body">
                <div id="notes-editor" class="notes-markdown-editor"
                     data-base="{base}" data-client-uuid="{uuid}"
                     data-denorm-url="{base}/notes/{id}/denorm"
                     data-title-input="#notes-title-input"></div>
            </div>
            <form method="POST" action="{base}/notes/{id}/delete" class="notes-delete-form"
                  onsubmit="return confirm('{delete_confirm}')">
                <button type="submit" class="btn btn-danger">{delete}</button>
            </form>
        </div>
        <script src="{base}/static/notes-vendor.bundle.js?v={sv}"></script>
        <script src="{base}/static/notes-tiptap-bootstrap.js?v={sv}"></script>"##,
        id = note.id,
        uuid = html_attr_escape(&note.client_uuid),
        untitled = t.untitled,
        back = t.back,
        delete = t.delete,
        delete_confirm = t.delete_confirm,
    );

    Html(render_page(
        &format!(
            "Notes — {}",
            if note.title.is_empty() {
                t.untitled
            } else {
                &note.title
            }
        ),
        &notes_nav(base, "edit", lang),
        &body,
        &state.config,
        lang,
    ))
}

#[derive(Deserialize)]
struct DenormForm {
    title: String,
    body: String,
}

// Denormalises the current CRDT state back into `notes_notes.title`/`body` so
// the list view stays fresh. The CRDT is the source of truth for both; these
// columns exist only for the list render that can't realistically replay every
// Y.Doc on each request. The bootstrap calls this on a debounced timer +
// pagehide + visibilitychange (sendBeacon best-effort).
async fn denorm(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<DenormForm>,
) -> StatusCode {
    match sqlx::query(
        "UPDATE notes_notes SET title = ?, body = ?, updated_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(form.title.trim())
    .bind(&form.body)
    .bind(id)
    .bind(user_id.0)
    .execute(&state.pool)
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::warn!("notes denorm update failed for id={id}: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn delete(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    super::ops::delete_note(&state.pool, user_id.0, id)
        .await
        .ok();
    Redirect::to(&format!("{base}/notes"))
}

async fn toggle_pin(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    super::ops::toggle_pin(&state.pool, user_id.0, id)
        .await
        .ok();
    Redirect::to(&format!("{base}/notes/{id}/edit"))
}

async fn dictate(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let _ = (user_id, id);
    match handle_dictation(&state, multipart).await {
        Ok(text) => Html(text),
        Err(e) => Html(format!("Error: {e}")),
    }
}

async fn handle_dictation(
    state: &axum::extract::State<AppState>,
    mut multipart: axum::extract::Multipart,
) -> anyhow::Result<String> {
    use std::io::Write;

    let mut audio_data = None;
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("audio") {
            audio_data = Some(field.bytes().await?);
        }
    }

    let data = audio_data.ok_or_else(|| anyhow::anyhow!("No audio data"))?;

    let tmp_dir = std::path::PathBuf::from("data/notes_tmp");
    std::fs::create_dir_all(&tmp_dir)?;
    let input_path = tmp_dir.join(format!("dictate-{}.webm", uuid::Uuid::new_v4()));
    let mut f = std::fs::File::create(&input_path)?;
    f.write_all(&data)?;
    drop(f);

    let wav_path = myapps_core::services::whisper::convert_to_wav(&input_path).await?;
    let text = myapps_core::services::whisper::transcribe(&state.config, &wav_path, "base").await?;

    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&wav_path);

    Ok(text)
}

// ── Helpers ─────────────────────────────────────────────────

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_attr_escape(s: &str) -> String {
    html_escape(s).replace('\'', "&#39;")
}
