use axum::extract::{Multipart, Path};
use axum::{Extension, Router, response::Html, routing::get, routing::post};
use std::path::PathBuf;

use super::dashboard::voice_nav;
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::layout::render_page;
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/new", get(new_form))
        .route("/upload", post(upload))
        .route("/jobs/list", get(jobs_list_partial))
        .route("/jobs/{job_id}", get(job_detail))
        .route("/jobs/{job_id}/delete", post(delete_job))
        .route("/jobs/{job_id}/retry", post(retry_job))
}

/// Upload directory relative to working dir.
fn upload_dir() -> PathBuf {
    PathBuf::from("data/voice_uploads")
}

/// Render `<option>` tags for the model selector.
/// If `selected` is provided, that model is pre-selected; otherwise the first is.
fn render_model_options(models: &[String], selected: Option<&str>, lang: Lang) -> String {
    if models.is_empty() {
        let t = i18n::t(lang);
        return format!(
            r#"<option value="" disabled>{}</option>"#,
            t.vt_new_no_models
        );
    }
    let mut html = String::new();
    for (i, m) in models.iter().enumerate() {
        let sel = match selected {
            Some(s) => s == m,
            None => i == 0,
        };
        let sel_attr = if sel { " selected" } else { "" };
        html.push_str(&format!(r#"<option value="{m}"{sel_attr}>{m}</option>"#));
    }
    html
}

async fn new_form(
    state: axum::extract::State<AppState>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let models = state.config.available_whisper_models();
    let model_options = render_model_options(&models, None, lang);

    let body = format!(
        r##"<div class="page-header">
            <h1>{new_title}</h1>
            <p>{new_subtitle}</p>
        </div>
        <div class="card">
            <form hx-post="{base}/voice/upload"
                  hx-encoding="multipart/form-data"
                  hx-target="#upload-result"
                  hx-swap="innerHTML"
                  class="voice-upload-form">
                <div class="form-group">
                    <label for="audio-file">{audio_file}</label>
                    <input type="file" id="audio-file" name="audio"
                           accept="audio/*,.wav,.mp3,.ogg,.webm,.m4a,.flac">
                </div>
                <div class="form-group">
                    <label for="model">{model_label}</label>
                    <select id="model" name="model">
                        {model_options}
                    </select>
                </div>
                <button type="submit" class="btn btn-primary">{upload_btn}</button>
            </form>
            <div id="upload-result"></div>
        </div>
        <div class="card" style="margin-top:1rem;">
            <h2>{record}</h2>
            <div id="recorder">
                <button id="rec-start" class="btn btn-primary" onclick="startRecording()">{start}</button>
                <button id="rec-stop" class="btn" onclick="stopRecording()" disabled>{stop}</button>
                <span id="rec-status"></span>
            </div>
            <div id="rec-result"></div>
        </div>
        <script>
        let mediaRecorder, audioChunks = [];
        async function startRecording() {{
            const stream = await navigator.mediaDevices.getUserMedia({{ audio: true }});
            mediaRecorder = new MediaRecorder(stream);
            audioChunks = [];
            mediaRecorder.ondataavailable = e => audioChunks.push(e.data);
            mediaRecorder.onstop = async () => {{
                stream.getTracks().forEach(t => t.stop());
                const blob = new Blob(audioChunks, {{ type: 'audio/webm' }});
                const form = new FormData();
                form.append('audio', blob, 'recording.webm');
                form.append('model', document.getElementById('model').value);
                const resp = await fetch('{base}/voice/upload', {{ method: 'POST', body: form }});
                document.getElementById('rec-result').innerHTML = await resp.text();
            }};
            mediaRecorder.start();
            document.getElementById('rec-start').disabled = true;
            document.getElementById('rec-stop').disabled = false;
            document.getElementById('rec-status').textContent = '{recording}';
        }}
        function stopRecording() {{
            mediaRecorder.stop();
            document.getElementById('rec-start').disabled = false;
            document.getElementById('rec-stop').disabled = true;
            document.getElementById('rec-status').textContent = '{processing}';
        }}
        </script>"##,
        new_title = t.vt_new_title,
        new_subtitle = t.vt_new_subtitle,
        audio_file = t.vt_new_audio_file,
        model_label = t.vt_new_model,
        upload_btn = t.vt_new_upload_btn,
        record = t.vt_new_record,
        start = t.vt_new_start,
        stop = t.vt_new_stop,
        recording = t.vt_new_recording,
        processing = t.vt_new_processing,
    );
    Html(render_page(
        &format!("VoiceToText — {}", t.vt_new),
        &voice_nav(base, "new", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn upload(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    mut multipart: Multipart,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let dir = upload_dir();
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return Html(format!(
            r#"<p class="error">Failed to create upload dir: {e}</p>"#
        ));
    }

    let available = state.config.available_whisper_models();
    let default_model = available
        .first()
        .cloned()
        .unwrap_or_else(|| "base".to_string());

    let mut audio_data: Option<(String, Vec<u8>)> = None;
    let mut model = default_model;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "model" {
            if let Ok(val) = field.text().await
                && available.contains(&val)
            {
                model = val;
            }
        } else if name == "audio" {
            let filename = field.file_name().unwrap_or("upload.wav").to_string();
            if let Ok(bytes) = field.bytes().await {
                audio_data = Some((filename, bytes.to_vec()));
            }
        }
    }

    let Some((original_filename, bytes)) = audio_data else {
        return Html(r#"<p class="error">No audio file provided.</p>"#.to_string());
    };

    if bytes.is_empty() {
        return Html(r#"<p class="error">Empty audio file.</p>"#.to_string());
    }

    // Save file with a unique name
    let file_id = uuid::Uuid::new_v4();
    let ext = original_filename.rsplit('.').next().unwrap_or("wav");
    let stored_name = format!("{file_id}.{ext}");
    let stored_path = dir.join(&stored_name);

    if let Err(e) = tokio::fs::write(&stored_path, &bytes).await {
        return Html(format!(r#"<p class="error">Failed to save file: {e}</p>"#));
    }

    // Create job row
    let result = sqlx::query(
        "INSERT INTO voice_jobs (user_id, original_filename, audio_path, model_used)
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id.0)
    .bind(&original_filename)
    .bind(stored_path.to_string_lossy().as_ref())
    .bind(&model)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => Html(format!(
            r#"<p class="success">Queued <strong>{original_filename}</strong> for transcription (model: {model}).
               <a href="{base}/voice">{view_jobs}</a></p>"#,
            view_jobs = t.vt_detail_view_jobs,
        )),
        Err(e) => Html(format!(r#"<p class="error">Database error: {e}</p>"#)),
    }
}

/// HTMX partial: refreshes the job list tbody for auto-polling.
async fn jobs_list_partial(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    Html(render_jobs_tbody(&state.pool, &state.config.base_path, user_id, lang).await)
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
            <button type="submit" class="btn-icon" title="Delete">&times;</button>
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
        completed = j.completed_at.as_deref().unwrap_or("—"),
    )
}

/// Shared helper: fetch jobs and render tbody rows.
async fn render_jobs_tbody(
    pool: &sqlx::SqlitePool,
    base: &str,
    user_id: UserId,
    lang: Lang,
) -> String {
    let jobs: Vec<JobRow> = sqlx::query_as(
        "SELECT id, status, original_filename, model_used, created_at, completed_at
         FROM voice_jobs
         WHERE user_id = ?
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(user_id.0)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut rows = String::new();
    for j in &jobs {
        rows.push_str(&render_job_row(j, base, lang));
    }
    rows
}

#[derive(sqlx::FromRow)]
struct JobDetail {
    id: i64,
    status: String,
    original_filename: String,
    transcription: Option<String>,
    error_message: Option<String>,
    model_used: String,
    duration_secs: Option<f64>,
    created_at: String,
    completed_at: Option<String>,
}

async fn job_detail(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(job_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let job: Option<JobDetail> = sqlx::query_as(
        "SELECT id, status, original_filename, transcription, error_message,
                model_used, duration_secs, created_at, completed_at
         FROM voice_jobs
         WHERE id = ? AND user_id = ?",
    )
    .bind(job_id)
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(j) = job else {
        return Html(render_page(
            "VoiceToText — Not Found",
            &voice_nav(base, "jobs", lang),
            &format!(
                r#"<div class="page-header"><h1>{}</h1></div>"#,
                t.vt_detail_not_found
            ),
            &state.config,
            lang,
        ));
    };

    let transcription_html = match &j.transcription {
        Some(text) => format!(
            r#"<div class="form-group">
                <label>{label}</label>
                <div class="voice-transcription">{text}</div>
            </div>"#,
            label = t.vt_detail_transcription,
        ),
        None => String::new(),
    };

    let error_html = match &j.error_message {
        Some(msg) => format!(r#"<p class="error">{msg}</p>"#),
        None => String::new(),
    };

    let duration_html = match j.duration_secs {
        Some(d) => format!("{d:.1}s"),
        None => "—".to_string(),
    };

    // Build retry form with other available models
    let models = state.config.available_whisper_models();
    let other_models: Vec<&String> = models.iter().filter(|m| **m != j.model_used).collect();
    let retry_html = if other_models.is_empty() {
        String::new()
    } else {
        let opts = render_model_options(
            &other_models
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>(),
            None,
            lang,
        );
        format!(
            r#"<div class="form-group">
                <label>{retranscribe}</label>
                <form action="{base}/voice/jobs/{id}/retry" method="post" class="voice-retry-form">
                    <select name="model">{opts}</select>
                    <button type="submit" class="btn btn-primary">{retranscribe_btn}</button>
                </form>
            </div>"#,
            id = j.id,
            retranscribe = t.vt_detail_retranscribe,
            retranscribe_btn = t.vt_detail_retranscribe_btn,
        )
    };

    let body = format!(
        r##"<div class="page-header">
            <h1>Job #{id}</h1>
            <p><a href="{base}/voice">&larr; {back}</a></p>
        </div>
        <div class="card">
            <div class="form-group">
                <label>{file_label}</label>
                <p>{filename}</p>
            </div>
            <div class="form-group">
                <label>{status_label}</label>
                <p>{status}</p>
            </div>
            <div class="form-group">
                <label>{model_label}</label>
                <p>{model}</p>
            </div>
            <div class="form-group">
                <label>{time_label}</label>
                <p>{duration}</p>
            </div>
            <div class="form-group">
                <label>{created_label}</label>
                <p>{created}</p>
            </div>
            <div class="form-group">
                <label>{completed_label}</label>
                <p>{completed}</p>
            </div>
            {error_html}
            {transcription_html}
            {retry_html}
        </div>"##,
        id = j.id,
        back = t.vt_detail_back,
        file_label = t.vt_detail_file,
        filename = j.original_filename,
        status_label = t.vt_detail_status,
        status = j.status,
        model_label = t.vt_detail_model,
        model = j.model_used,
        time_label = t.vt_detail_time,
        duration = duration_html,
        created_label = t.vt_detail_created,
        created = j.created_at,
        completed_label = t.vt_detail_completed,
        completed = j.completed_at.as_deref().unwrap_or("—"),
    );
    Html(render_page(
        &format!("VoiceToText — Job #{}", j.id),
        &voice_nav(base, "jobs", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn delete_job(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(job_id): Path<i64>,
) -> Html<String> {
    // Delete the audio file if it exists
    let path: Option<(String,)> =
        sqlx::query_as("SELECT audio_path FROM voice_jobs WHERE id = ? AND user_id = ?")
            .bind(job_id)
            .bind(user_id.0)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if let Some((audio_path,)) = &path {
        let _ = tokio::fs::remove_file(audio_path).await;
        // Also remove the converted WAV if it exists
        let wav = std::path::Path::new(audio_path).with_extension("16k.wav");
        let _ = tokio::fs::remove_file(wav).await;
    }

    sqlx::query("DELETE FROM voice_jobs WHERE id = ? AND user_id = ?")
        .bind(job_id)
        .bind(user_id.0)
        .execute(&state.pool)
        .await
        .ok();

    Html(render_jobs_tbody(&state.pool, &state.config.base_path, user_id, lang).await)
}

#[derive(serde::Deserialize)]
struct RetryForm {
    model: String,
}

async fn retry_job(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(job_id): Path<i64>,
    axum::Form(form): axum::Form<RetryForm>,
) -> axum::response::Redirect {
    let base = &state.config.base_path;
    let available = state.config.available_whisper_models();

    if available.contains(&form.model) {
        // Look up the original job to reuse its audio file
        let original: Option<(String, String)> = sqlx::query_as(
            "SELECT original_filename, audio_path FROM voice_jobs WHERE id = ? AND user_id = ?",
        )
        .bind(job_id)
        .bind(user_id.0)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((filename, audio_path)) = original {
            sqlx::query(
                "INSERT INTO voice_jobs (user_id, original_filename, audio_path, model_used)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(user_id.0)
            .bind(&filename)
            .bind(&audio_path)
            .bind(&form.model)
            .execute(&state.pool)
            .await
            .ok();
        }
    }

    axum::response::Redirect::to(&format!("{base}/voice"))
}
