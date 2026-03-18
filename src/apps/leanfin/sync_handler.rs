use axum::http::HeaderValue;
use axum::{
    Extension, Router,
    response::{Html, IntoResponse},
    routing::post,
};

use super::services::sync;
use crate::auth::UserId;
use crate::i18n::{self, Lang};
use crate::routes::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/sync", post(trigger_sync))
}

async fn trigger_sync(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> impl IntoResponse {
    let t = i18n::t(lang);
    let result = sync::run_for_user(&state.pool, &state.config, user_id.0).await;

    let btn = sync_button(&state.config.base_path, lang);

    let warning_html = if result.reconciliation_warnings.is_empty() {
        String::new()
    } else {
        let warnings = result.reconciliation_warnings.join("<br>");
        format!(r#"<div class="reconciliation-alert">{warnings}</div>"#)
    };

    let html = if result.errors.is_empty() {
        let msg = if result.accounts_synced == 0 {
            t.lf_sync_no_accounts.to_string()
        } else {
            format!(
                "Synced {} new transaction{}",
                result.total_new,
                if result.total_new == 1 { "" } else { "s" },
            )
        };
        format!(
            r##"{btn}
            <span class="sync-status sync-status-ok">{msg}</span>
            {warning_html}"##,
        )
    } else {
        let error_summary = result.errors.join("; ");
        let msg = if result.accounts_synced > 0 {
            format!(
                "Synced {} new, but errors: {}",
                result.total_new, error_summary,
            )
        } else {
            format!("Sync failed: {error_summary}")
        };
        format!(
            r##"{btn}
            <span class="sync-status sync-status-error">{msg}</span>
            {warning_html}"##,
        )
    };

    // HX-Trigger tells HTMX to fire a "sync-done" event on the page,
    // which the txn-table and account-grid can listen for to refresh.
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("HX-Trigger", HeaderValue::from_static("sync-done"));
    response
}

/// Render the sync button HTML. Shared by the handler and the page templates.
pub fn sync_button(base: &str, lang: Lang) -> String {
    let t = i18n::t(lang);
    format!(
        r##"<button class="btn btn-secondary btn-sm sync-btn"
                hx-post="{base}/leanfin/sync"
                hx-target="#sync-container"
                hx-swap="innerHTML"
                hx-indicator="#sync-spinner">
            <span class="sync-icon" id="sync-spinner">&#x21bb;</span>
            {sync}
        </button>"##,
        sync = t.lf_txn_sync,
    )
}
