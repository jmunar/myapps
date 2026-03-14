use axum::extract::{Multipart, Path};
use axum::{Extension, Router, response::Html, routing::get, routing::post};
use std::path::PathBuf;

use crate::auth::UserId;
use crate::layout::render_page;
use crate::routes::AppState;
use super::dashboard::voice_nav;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/new", get(new_form))
        .route("/upload", post(upload))
        .route("/jobs/list", get(jobs_list_partial))
        .route("/jobs/{job_id}", get(job_detail))
}

/// Upload directory relative to working dir.
fn upload_dir() -> PathBuf {
    PathBuf::from("data/voice_uploads")
}

async fn new_form(
    state: axum::extract::State<AppState>,
) -> Html<String> {
    let base = &state.config.base_path;

    let body = format!(
        r##"<div class="page-header">
            <h1>New Transcription</h1>
            <p>Upload an audio file or record from your microphone</p>
        </div>
        <div class="card">
            <form hx-post="{base}/voice/upload"
                  hx-encoding="multipart/form-data"
                  hx-target="#upload-result"
                  hx-swap="innerHTML"
                  class="voice-upload-form">
                <div class="form-group">
                    <label for="audio-file">Audio file</label>
                    <input type="file" id="audio-file" name="audio"
                           accept="audio/*,.wav,.mp3,.ogg,.webm,.m4a,.flac">
                </div>
                <div class="form-group">
                    <label for="model">Model</label>
                    <select id="model" name="model">
                        <option value="base" selected>Base (better accuracy, slower)</option>
                        <option value="tiny">Tiny (faster, less accurate)</option>
                    </select>
                </div>
                <button type="submit" class="btn btn-primary">Upload &amp; Transcribe</button>
            </form>
            <div id="upload-result"></div>
        </div>
        <div class="card" style="margin-top:1rem;">
            <h2>Record Audio</h2>
            <div id="recorder">
                <button id="rec-start" class="btn btn-primary" onclick="startRecording()">Start Recording</button>
                <button id="rec-stop" class="btn" onclick="stopRecording()" disabled>Stop Recording</button>
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
            document.getElementById('rec-status').textContent = 'Recording…';
        }}
        function stopRecording() {{
            mediaRecorder.stop();
            document.getElementById('rec-start').disabled = false;
            document.getElementById('rec-stop').disabled = true;
            document.getElementById('rec-status').textContent = 'Processing…';
        }}
        </script>"##
    );
    Html(render_page(
        "VoiceToText — New",
        &voice_nav(base, "new"),
        &body,
        base,
    ))
}

async fn upload(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    mut multipart: Multipart,
) -> Html<String> {
    let base = &state.config.base_path;
    let dir = upload_dir();
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return Html(format!(r#"<p class="error">Failed to create upload dir: {e}</p>"#));
    }

    let mut audio_data: Option<(String, Vec<u8>)> = None;
    let mut model = "base".to_string();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "model" {
            if let Ok(val) = field.text().await {
                if val == "tiny" || val == "base" {
                    model = val;
                }
            }
        } else if name == "audio" {
            let filename = field
                .file_name()
                .unwrap_or("upload.wav")
                .to_string();
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
    let ext = original_filename
        .rsplit('.')
        .next()
        .unwrap_or("wav");
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
               <a href="{base}/voice">View jobs</a></p>"#
        )),
        Err(e) => Html(format!(r#"<p class="error">Database error: {e}</p>"#)),
    }
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

/// HTMX partial: refreshes the job list tbody for auto-polling.
async fn jobs_list_partial(
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
        let status_class = match j.status.as_str() {
            "done" => "status-done",
            "failed" => "status-failed",
            "processing" => "status-processing",
            _ => "status-pending",
        };
        let view_link = if j.status == "done" {
            format!(r#"<a href="{base}/voice/jobs/{id}">View</a>"#, id = j.id)
        } else {
            String::new()
        };
        rows.push_str(&format!(
            r#"<tr>
                <td>{filename}</td>
                <td><span class="voice-status {status_class}">{status}</span></td>
                <td>{model}</td>
                <td>{created}</td>
                <td>{completed}</td>
                <td>{view_link}</td>
            </tr>"#,
            filename = j.original_filename,
            status = j.status,
            model = j.model_used,
            created = j.created_at,
            completed = j.completed_at.as_deref().unwrap_or("—"),
        ));
    }
    Html(rows)
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
    Path(job_id): Path<i64>,
) -> Html<String> {
    let base = &state.config.base_path;

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
            &voice_nav(base, "jobs"),
            r#"<div class="page-header"><h1>Job not found</h1></div>"#,
            base,
        ));
    };

    let transcription_html = match &j.transcription {
        Some(text) => format!(
            r#"<div class="form-group">
                <label>Transcription</label>
                <div class="voice-transcription">{text}</div>
            </div>"#
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

    let body = format!(
        r##"<div class="page-header">
            <h1>Job #{id}</h1>
            <p><a href="{base}/voice">&larr; Back to jobs</a></p>
        </div>
        <div class="card">
            <div class="form-group">
                <label>File</label>
                <p>{filename}</p>
            </div>
            <div class="form-group">
                <label>Status</label>
                <p>{status}</p>
            </div>
            <div class="form-group">
                <label>Model</label>
                <p>{model}</p>
            </div>
            <div class="form-group">
                <label>Processing time</label>
                <p>{duration}</p>
            </div>
            <div class="form-group">
                <label>Created</label>
                <p>{created}</p>
            </div>
            <div class="form-group">
                <label>Completed</label>
                <p>{completed}</p>
            </div>
            {error_html}
            {transcription_html}
        </div>"##,
        id = j.id,
        filename = j.original_filename,
        status = j.status,
        model = j.model_used,
        duration = duration_html,
        created = j.created_at,
        completed = j.completed_at.as_deref().unwrap_or("—"),
    );
    Html(render_page(
        &format!("VoiceToText — Job #{}", j.id),
        &voice_nav(base, "jobs"),
        &body,
        base,
    ))
}
