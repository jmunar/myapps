use axum::{
    Extension, Form, Router,
    extract::Path,
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
        .route("/{id}/save", post(save))
        .route("/{id}/delete", post(delete))
        .route("/{id}/toggle-pin", post(toggle_pin))
        .route("/{id}/dictate", post(dictate))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct NoteRow {
    id: i64,
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
        "SELECT id, title, body, pinned, created_at, updated_at FROM notes_notes WHERE user_id = ? ORDER BY pinned DESC, updated_at DESC",
    )
    .bind(user_id.0)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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
        "SELECT id, title, body, pinned, created_at, updated_at FROM notes_notes WHERE id = ? AND user_id = ?",
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
    let whisper_available = state.config.whisper_available();

    let dictate_btn = if whisper_available {
        format!(
            r##"<button type="button" id="notes-dictate-btn" class="btn btn-secondary" title="{dictate}">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/></svg>
                {dictate}
            </button>"##,
            dictate = t.dictate,
        )
    } else {
        String::new()
    };

    let sv = &state.config.static_version;

    let body = format!(
        r#"<div class="notes-editor-container">
            <form method="POST" action="{base}/notes/{id}/save" id="notes-form">
                <div class="notes-editor-toolbar">
                    <input type="text" name="title" value="{title}" placeholder="{untitled}"
                           class="notes-title-input" autocomplete="off">
                    <div class="notes-editor-actions">
                        {dictate_btn}
                        <button type="submit" formaction="{base}/notes/{id}/toggle-pin" class="btn btn-secondary">{pin_label}</button>
                        <button type="submit" class="btn btn-primary">{save}</button>
                        <a href="{base}/notes" class="btn btn-secondary">{back}</a>
                    </div>
                </div>
                <div class="notes-editor-body">
                    <div id="notes-editor" class="notes-markdown-editor" contenteditable="true"
                         data-base="{base}" data-note-id="{id}" data-whisper="{whisper}"
                         data-t-dictating="{t_dictating}" data-t-transcribing="{t_transcribing}">{body_html}</div>
                    <textarea name="body" id="notes-raw" style="display:none">{body_raw}</textarea>
                </div>
            </form>
            <form method="POST" action="{base}/notes/{id}/delete" class="notes-delete-form"
                  onsubmit="return confirm('{delete_confirm}')">
                <button type="submit" class="btn btn-danger">{delete}</button>
            </form>
        </div>
        <script src="{base}/static/notes-editor.js?v={sv}"></script>"#,
        id = note.id,
        title = html_attr_escape(&note.title),
        body_html = markdown_to_editor_html(&note.body),
        body_raw = html_escape(&note.body),
        untitled = t.untitled,
        save = t.save,
        back = t.back,
        delete = t.delete,
        delete_confirm = t.delete_confirm,
        whisper = whisper_available,
        t_dictating = t.dictating,
        t_transcribing = t.transcribing,
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
struct SaveForm {
    title: String,
    body: String,
}

async fn save(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(id): Path<i64>,
    Form(form): Form<SaveForm>,
) -> impl IntoResponse {
    let base = &state.config.base_path;
    super::ops::update_note(
        &state.pool,
        user_id.0,
        id,
        form.title.trim(),
        form.body.trim(),
    )
    .await
    .ok();
    Redirect::to(&format!("{base}/notes/{id}/edit"))
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

    let tmp_dir = std::env::temp_dir().join("myapps-notes");
    std::fs::create_dir_all(&tmp_dir)?;
    let input_path = tmp_dir.join(format!("dictate-{}.webm", uuid::Uuid::new_v4()));
    let mut f = std::fs::File::create(&input_path)?;
    f.write_all(&data)?;
    drop(f);

    let wav_path = myapps_core::services::whisper::convert_to_wav(&input_path).await?;
    let text = myapps_core::services::whisper::transcribe(&state.config, &wav_path, "base").await?;

    // Clean up temp files
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

/// Convert markdown to minimal HTML for the contenteditable editor.
/// This is a simple server-side pre-render — the JS editor handles live editing.
fn markdown_to_editor_html(md: &str) -> String {
    let mut html = String::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut in_list = false;
    let mut list_ordered = false;

    for line in md.lines() {
        // Code blocks
        if line.trim_start().starts_with("```") {
            if in_code_block {
                html.push_str(&format!(
                    r#"<pre class="notes-code-block" data-lang="{}"><code>{}</code></pre>"#,
                    html_escape(&code_lang),
                    html_escape(&code_lines.join("\n")),
                ));
                code_lines.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                close_list(&mut html, &mut in_list, list_ordered);
                in_code_block = true;
                code_lang = line.trim_start().trim_start_matches('`').to_string();
            }
            continue;
        }
        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        let trimmed = line.trim();

        // Empty line — close list
        if trimmed.is_empty() {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str("<p><br></p>");
            continue;
        }

        // Headings
        if let Some(rest) = trimmed.strip_prefix("### ") {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str(&format!("<h3>{}</h3>", inline_md(&html_escape(rest))));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str(&format!("<h2>{}</h2>", inline_md(&html_escape(rest))));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str(&format!("<h1>{}</h1>", inline_md(&html_escape(rest))));
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str("<hr>");
            continue;
        }

        // Blockquote
        if let Some(rest) = trimmed.strip_prefix("> ") {
            close_list(&mut html, &mut in_list, list_ordered);
            html.push_str(&format!(
                "<blockquote>{}</blockquote>",
                inline_md(&html_escape(rest))
            ));
            continue;
        }

        // Unordered list (- or *)
        if trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("* [x] ")
            || trimmed.starts_with("* [ ] ")
        {
            if !in_list {
                html.push_str("<ul>");
                in_list = true;
                list_ordered = false;
            }
            let checked = trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]");
            let rest = &trimmed[6..];
            let check = if checked { "&#9745; " } else { "&#9744; " };
            html.push_str(&format!(
                "<li>{}{}</li>",
                check,
                inline_md(&html_escape(rest))
            ));
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            if !in_list {
                html.push_str("<ul>");
                in_list = true;
                list_ordered = false;
            }
            html.push_str(&format!("<li>{}</li>", inline_md(&html_escape(rest))));
            continue;
        }

        // Ordered list
        if let Some(pos) = trimmed.find(". ") {
            let num_part = &trimmed[..pos];
            if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
                let rest = &trimmed[pos + 2..];
                if !in_list || !list_ordered {
                    close_list(&mut html, &mut in_list, list_ordered);
                    html.push_str("<ol>");
                    in_list = true;
                    list_ordered = true;
                }
                html.push_str(&format!("<li>{}</li>", inline_md(&html_escape(rest))));
                continue;
            }
        }

        // Regular paragraph
        close_list(&mut html, &mut in_list, list_ordered);
        html.push_str(&format!("<p>{}</p>", inline_md(&html_escape(trimmed))));
    }

    // Close any open code block
    if in_code_block {
        html.push_str(&format!(
            r#"<pre class="notes-code-block" data-lang="{}"><code>{}</code></pre>"#,
            html_escape(&code_lang),
            html_escape(&code_lines.join("\n")),
        ));
    }

    close_list(&mut html, &mut in_list, list_ordered);
    html
}

fn close_list(html: &mut String, in_list: &mut bool, ordered: bool) {
    if *in_list {
        if ordered {
            html.push_str("</ol>");
        } else {
            html.push_str("</ul>");
        }
        *in_list = false;
    }
}

/// Convert inline markdown (bold, italic, code, links) to HTML.
fn inline_md(s: &str) -> String {
    let mut out = s.to_string();
    // Bold: **text**
    while let Some(start) = out.find("**") {
        if let Some(end) = out[start + 2..].find("**") {
            let inner = &out[start + 2..start + 2 + end].to_string();
            out = format!(
                "{}<strong>{}</strong>{}",
                &out[..start],
                inner,
                &out[start + 2 + end + 2..]
            );
        } else {
            break;
        }
    }
    // Italic: *text*
    while let Some(start) = out.find('*') {
        if let Some(end) = out[start + 1..].find('*') {
            let inner = &out[start + 1..start + 1 + end].to_string();
            out = format!(
                "{}<em>{}</em>{}",
                &out[..start],
                inner,
                &out[start + 1 + end + 1..]
            );
        } else {
            break;
        }
    }
    // Inline code: `text`
    while let Some(start) = out.find('`') {
        if let Some(end) = out[start + 1..].find('`') {
            let inner = &out[start + 1..start + 1 + end].to_string();
            out = format!(
                "{}<code>{}</code>{}",
                &out[..start],
                inner,
                &out[start + 1 + end + 1..]
            );
        } else {
            break;
        }
    }
    // Links: [text](url) — only render text, not the URL (security)
    while let Some(bracket_start) = out.find('[') {
        if let Some(bracket_end) = out[bracket_start..].find("](") {
            let abs_bracket_end = bracket_start + bracket_end;
            if let Some(paren_end) = out[abs_bracket_end + 2..].find(')') {
                let text = &out[bracket_start + 1..abs_bracket_end].to_string();
                let url = &out[abs_bracket_end + 2..abs_bracket_end + 2 + paren_end].to_string();
                out = format!(
                    "{}<a href=\"{}\">{}</a>{}",
                    &out[..bracket_start],
                    url,
                    text,
                    &out[abs_bracket_end + 2 + paren_end + 1..]
                );
            } else {
                break;
            }
        } else {
            break;
        }
    }
    out
}
