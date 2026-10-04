//! FileClipboard routes: drop zone, file list, streaming upload, download.

use axum::extract::{DefaultBodyLimit, Extension, Multipart, Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::{Form, Router, routing::get, routing::post};
use serde::Deserialize;
use tower_http::services::ServeFile;

use myapps_core::auth::UserId;
use myapps_core::components::html_escape;
use myapps_core::i18n::Lang;
use myapps_core::layout::{NavItem, render_page};
use myapps_core::routes::AppState;

use crate::i18n;
use crate::ops;
use crate::storage::{self, Limits, UploadError};

/// Drag-and-drop uploader. Kept in its own file so the Rust side never has to
/// brace-escape a page of JavaScript.
const UPLOAD_JS: &str = include_str!("../static/upload.js");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/files/list", get(list_partial))
        .route("/files/{file_id}/download", get(download))
        .route("/files/{file_id}/delete", post(delete))
        .route("/settings", post(update_settings))
        // axum caps request bodies at 2 MB by default, which `Multipart`
        // inherits. Disabled here only — the real ceiling is enforced while
        // streaming, where an oversized upload is aborted rather than buffered.
        .route("/upload", post(upload).layer(DefaultBodyLimit::disable()))
}

pub fn clipboard_nav(base: &str, active: &str, lang: Lang) -> Vec<NavItem> {
    let t = i18n::t(lang);
    myapps_core::layout::app_nav(
        base,
        "/file_clipboard",
        "FileClipboard",
        active,
        lang,
        &[("", t.files, "files")],
    )
}

// ── Pages ───────────────────────────────────────────────────

async fn index(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let retention = ops::retention_days(
        &state.pool,
        user_id.0,
        state.config.file_clipboard_retention_days,
    )
    .await;
    let list = render_list(&state, user_id.0, lang).await;

    let body = format!(
        r##"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>

        <div class="card">
            <div id="fc-dropzone" class="fc-dropzone" data-base="{base}" data-err-failed="{err_failed}">
                <div class="fc-dropzone-title">{drop_title}</div>
                <div class="fc-dropzone-hint">{drop_hint} <button type="button" id="fc-browse" class="fc-browse">{drop_browse}</button></div>
                <input type="file" id="fc-file-input" multiple hidden>
            </div>
            <div id="fc-progress" class="fc-progress"></div>
            <div id="fc-upload-status" class="fc-upload-status"></div>
        </div>

        <div class="card fc-settings">
            <form hx-post="{base}/file_clipboard/settings"
                  hx-target="#fc-settings-status"
                  hx-swap="innerHTML"
                  class="fc-settings-form">
                <label for="fc-retention">{retention_label}</label>
                <input type="number" id="fc-retention" name="retention_days"
                       min="{min_days}" max="{max_days}" value="{retention}">
                <span class="fc-settings-hint">{retention_hint}</span>
                <button type="submit" class="btn">{retention_save}</button>
                <span id="fc-settings-status" class="fc-settings-status"></span>
            </form>
        </div>

        <div id="fc-file-list"
             hx-get="{base}/file_clipboard/files/list"
             hx-trigger="fcRefresh from:body"
             hx-swap="innerHTML">{list}</div>

        <script>{js}</script>"##,
        title = t.title,
        subtitle = t.subtitle,
        drop_title = t.drop_title,
        drop_hint = t.drop_hint,
        drop_browse = t.drop_browse,
        err_failed = t.err_failed,
        retention_label = t.retention_label,
        retention_hint = t.retention_hint,
        retention_save = t.retention_save,
        min_days = ops::MIN_RETENTION_DAYS,
        max_days = ops::MAX_RETENTION_DAYS,
        js = UPLOAD_JS,
    );

    Html(render_page(
        &format!("FileClipboard — {}", t.files),
        &clipboard_nav(base, "files", lang),
        &body,
        &state.config,
        lang,
    ))
}

async fn list_partial(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    Html(render_list(&state, user_id.0, lang).await)
}

/// The file table plus the storage-usage footer. Shared by the full page, the
/// HTMX partial, and the upload response.
async fn render_list(state: &AppState, user_id: i64, lang: Lang) -> String {
    let t = i18n::t(lang);
    let base = &state.config.base_path;

    let files = ops::list_files(&state.pool, user_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("FileClipboard: listing files failed: {e:#}");
            Vec::new()
        });

    if files.is_empty() {
        return format!(
            r#"<div class="card"><p class="fc-empty">{}</p></div>"#,
            t.empty
        );
    }

    let mut rows = String::new();
    for f in &files {
        let id = f.id;
        // `original_name` is entirely user-controlled and `format!` does no
        // escaping of its own.
        let name = html_escape(&f.original_name);
        rows.push_str(&format!(
            r##"<tr>
                <td class="fc-name"><a href="{base}/file_clipboard/files/{id}/download">{name}</a></td>
                <td class="fc-size" data-label="{col_size}">{size}</td>
                <td class="fc-added" data-label="{col_added}">{created}</td>
                <td class="fc-expires" data-label="{col_expires}">{expires}</td>
                <td class="fc-actions">
                    <a class="btn-icon" href="{base}/file_clipboard/files/{id}/download" title="{download}">&darr;</a>
                    <form hx-post="{base}/file_clipboard/files/{id}/delete"
                          hx-target="#fc-file-list" hx-swap="innerHTML"
                          hx-confirm="{confirm}">
                        <button type="submit" class="btn-icon" title="{delete}">&times;</button>
                    </form>
                </td>
            </tr>"##,
            size = storage::fmt_size(f.size_bytes),
            created = f.created_at,
            expires = f.expires_at,
            col_size = t.col_size,
            col_added = t.col_added,
            col_expires = t.col_expires,
            download = t.download,
            delete = t.delete,
            confirm = t.delete_confirm,
        ));
    }

    let used = ops::usage_bytes(&state.pool, user_id).await.unwrap_or(0);
    let quota = state.config.file_clipboard_user_quota_bytes;

    format!(
        r##"<div class="card">
            <table class="fc-table table-cards">
                <thead><tr>
                    <th>{col_name}</th><th>{col_size}</th><th>{col_added}</th>
                    <th>{col_expires}</th><th>{col_actions}</th>
                </tr></thead>
                <tbody>{rows}</tbody>
            </table>
            <p class="fc-usage">{used} / {quota} {storage_used}</p>
        </div>"##,
        col_name = t.col_name,
        col_size = t.col_size,
        col_added = t.col_added,
        col_expires = t.col_expires,
        col_actions = t.col_actions,
        used = storage::fmt_size(used as i64),
        quota = storage::fmt_size(quota as i64),
        storage_used = t.storage_used,
    )
}

// ── Upload ──────────────────────────────────────────────────

fn error_fragment(status: StatusCode, message: &str) -> Response {
    (
        status,
        Html(format!(r#"<p class="error">{}</p>"#, html_escape(message))),
    )
        .into_response()
}

fn upload_error_response(e: UploadError, lang: Lang) -> Response {
    let t = i18n::t(lang);
    match e {
        UploadError::Empty => error_fragment(StatusCode::BAD_REQUEST, t.err_empty),
        UploadError::TooLarge => error_fragment(StatusCode::PAYLOAD_TOO_LARGE, t.err_too_large),
        UploadError::QuotaExceeded => error_fragment(StatusCode::PAYLOAD_TOO_LARGE, t.err_quota),
        UploadError::DiskFull => error_fragment(StatusCode::INSUFFICIENT_STORAGE, t.err_disk_full),
        UploadError::Read(msg) => {
            tracing::warn!("FileClipboard: upload stream failed: {msg}");
            error_fragment(StatusCode::BAD_REQUEST, t.err_failed)
        }
        UploadError::Io(err) => {
            tracing::error!("FileClipboard: writing upload failed: {err}");
            error_fragment(StatusCode::INTERNAL_SERVER_ERROR, t.err_failed)
        }
    }
}

/// Stream an upload to disk.
///
/// Bytes go straight from the multipart field to a `.part` file in fixed-size
/// chunks — never through a `Vec<u8>` — so a 5 GB upload does not have to fit
/// in the Odroid's RAM.
async fn upload(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    mut multipart: Multipart,
) -> Response {
    let t = i18n::t(lang);
    let cfg = &state.config;
    let base_dir = &cfg.file_clipboard_dir;

    let retention =
        ops::retention_days(&state.pool, user_id.0, cfg.file_clipboard_retention_days).await;
    let used = ops::usage_bytes(&state.pool, user_id.0).await.unwrap_or(0);

    let mut limits = Limits {
        max_file_bytes: cfg.file_clipboard_max_file_bytes,
        remaining_quota_bytes: cfg.file_clipboard_user_quota_bytes.saturating_sub(used),
        min_free_bytes: cfg.file_clipboard_min_free_bytes,
    };

    let mut stored_any = false;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("FileClipboard: malformed multipart: {e}");
                return error_fragment(StatusCode::BAD_REQUEST, t.err_failed);
            }
        };
        let mut field = field;

        let field_name = field.name().unwrap_or_default().to_string();
        if field_name != "file" {
            continue;
        }
        let original_name = storage::sanitize_name(field.file_name().unwrap_or("unnamed"));
        let content_type = field.content_type().map(|c| c.to_string());

        match storage::store_field(base_dir, user_id.0, &mut field, &limits).await {
            Ok(stored) => {
                if let Err(e) = ops::record_file(
                    &state.pool,
                    user_id.0,
                    &original_name,
                    &stored.stored_name,
                    stored.size_bytes,
                    content_type.as_deref(),
                    retention,
                )
                .await
                {
                    tracing::error!("FileClipboard: recording upload failed: {e:#}");
                    storage::remove(base_dir, user_id.0, &stored.stored_name).await;
                    return error_fragment(StatusCode::INTERNAL_SERVER_ERROR, t.err_failed);
                }
                limits.remaining_quota_bytes = limits
                    .remaining_quota_bytes
                    .saturating_sub(stored.size_bytes);
                stored_any = true;
            }
            Err(e) => return upload_error_response(e, lang),
        }
    }

    if !stored_any {
        return error_fragment(StatusCode::BAD_REQUEST, t.err_no_file);
    }

    Html(render_list(&state, user_id.0, lang).await).into_response()
}

// ── Download ────────────────────────────────────────────────

/// Percent-encode a filename for the `filename*` parameter (RFC 5987).
fn encode_rfc5987(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build an `attachment` disposition with an ASCII fallback plus a UTF-8 form.
fn attachment_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " ._-()[]".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        encode_rfc5987(name)
    )
}

/// Serve a stored file.
///
/// Delegated to `ServeFile` so byte-range requests work — a 5 GB download that
/// drops at 90% can resume instead of starting over. Everything about the
/// response is forced to be inert: these are arbitrary user-supplied bytes
/// served from the same origin as the session cookie, so an HTML or SVG file
/// rendered inline would be stored XSS against all of MyApps.
async fn download(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Path(file_id): Path<i64>,
    request: Request,
) -> Response {
    let row = ops::stored_name(&state.pool, user_id.0, file_id)
        .await
        .unwrap_or(None);

    let Some((stored, original)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let path = storage::file_path(&state.config.file_clipboard_dir, user_id.0, &stored);
    let mut service = ServeFile::new(&path);

    let response = match service.try_call(request).await {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("FileClipboard: serving {} failed: {e}", path.display());
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let mut response = response.map(axum::body::Body::new);
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    if let Ok(value) = HeaderValue::from_str(&attachment_disposition(&original)) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    } else {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
    }

    response
}

// ── Delete & settings ───────────────────────────────────────

async fn delete(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Path(file_id): Path<i64>,
) -> Html<String> {
    if let Err(e) = ops::delete_file(
        &state.pool,
        &state.config.file_clipboard_dir,
        user_id.0,
        file_id,
    )
    .await
    {
        tracing::error!("FileClipboard: deleting file {file_id} failed: {e:#}");
    }
    Html(render_list(&state, user_id.0, lang).await)
}

#[derive(Deserialize)]
struct SettingsForm {
    retention_days: i64,
}

async fn update_settings(
    State(state): State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    Form(form): Form<SettingsForm>,
) -> Response {
    let t = i18n::t(lang);

    if form.retention_days < ops::MIN_RETENTION_DAYS
        || form.retention_days > ops::MAX_RETENTION_DAYS
    {
        return error_fragment(StatusCode::BAD_REQUEST, t.retention_invalid);
    }

    if let Err(e) = ops::set_retention_days(&state.pool, user_id.0, form.retention_days).await {
        tracing::error!("FileClipboard: saving retention failed: {e:#}");
        return error_fragment(StatusCode::INTERNAL_SERVER_ERROR, t.err_failed);
    }

    // Re-stamped expiry dates change the list, so ask it to refresh itself.
    (
        [("HX-Trigger", "fcRefresh")],
        Html(format!(
            r#"<span class="success">{}</span>"#,
            t.retention_saved
        )),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_is_always_an_attachment() {
        let d = attachment_disposition("report.pdf");
        assert!(d.starts_with("attachment; "));
        assert!(d.contains(r#"filename="report.pdf""#));
    }

    #[test]
    fn disposition_encodes_non_ascii_and_quotes() {
        let d = attachment_disposition(r#"año "x".txt"#);
        assert!(!d.contains(r#""año"#));
        assert!(d.contains("filename*=UTF-8''a%C3%B1o"));
    }
}
