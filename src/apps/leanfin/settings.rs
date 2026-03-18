use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{Context, Result};
use axum::extract::Multipart;
use axum::{Extension, Router, response::Html, routing::get};
use sqlx::SqlitePool;

use super::dashboard::leanfin_nav;
use crate::auth::UserId;
use crate::config::Config;
use crate::i18n::{self, Lang};
use crate::layout::render_page;
use crate::routes::AppState;

// ── Credentials struct ──────────────────────────────────────────

pub struct EnableBankingCredentials {
    pub app_id: String,
    pub key_pem: String,
    pub redirect_uri: String,
}

// ── Encryption helpers ──────────────────────────────────────────

fn parse_encryption_key(hex_key: &str) -> Result<Key<Aes256Gcm>> {
    let bytes = hex::decode(hex_key).context("ENCRYPTION_KEY is not valid hex")?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "ENCRYPTION_KEY must be 32 bytes (64 hex chars), got {}",
            bytes.len()
        );
    }
    Ok(*Key::<Aes256Gcm>::from_slice(&bytes))
}

fn encrypt(plaintext: &[u8], hex_key: &str) -> Result<Vec<u8>> {
    let key = parse_encryption_key(hex_key)?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    // Prepend nonce (12 bytes) to ciphertext
    let mut result = nonce.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

fn decrypt(data: &[u8], hex_key: &str) -> Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("encrypted data too short");
    }
    let key = parse_encryption_key(hex_key)?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::from_slice(&data[..12]);
    let plaintext = cipher
        .decrypt(nonce, &data[12..])
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;
    Ok(plaintext)
}

// ── DB helpers ──────────────────────────────────────────────────

pub async fn get_credentials(
    pool: &SqlitePool,
    config: &Config,
    user_id: i64,
) -> Result<EnableBankingCredentials> {
    let encryption_key = config
        .encryption_key
        .as_deref()
        .context("ENCRYPTION_KEY not configured")?;

    let base_url = config
        .base_url
        .as_deref()
        .context("BASE_URL not configured")?;

    let row: (Option<String>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT enable_banking_app_id, enable_banking_key FROM leanfin_user_settings WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .context("Enable Banking credentials not configured")?;

    let app_id = row.0.context("Enable Banking App ID not configured")?;
    let encrypted_key = row.1.context("Enable Banking key not configured")?;

    let key_pem_bytes = decrypt(&encrypted_key, encryption_key)?;
    let key_pem = String::from_utf8(key_pem_bytes).context("invalid UTF-8 in decrypted key")?;

    Ok(EnableBankingCredentials {
        app_id,
        key_pem,
        redirect_uri: format!("{base_url}/leanfin/accounts/callback"),
    })
}

pub async fn has_credentials(pool: &SqlitePool, user_id: i64) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM leanfin_user_settings WHERE user_id = ? AND enable_banking_app_id IS NOT NULL AND enable_banking_key IS NOT NULL)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

// ── Routes ──────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(settings_form).post(settings_submit))
}

async fn settings_form(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
) -> Html<String> {
    let base = &state.config.base_path;
    let t = i18n::t(lang);

    let current_app_id: Option<String> = sqlx::query_scalar(
        "SELECT enable_banking_app_id FROM leanfin_user_settings WHERE user_id = ?",
    )
    .bind(user_id.0)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();

    let has_key = has_credentials(&state.pool, user_id.0).await;

    let app_id_value = current_app_id.as_deref().unwrap_or("");
    let key_status = if has_key {
        format!(r#"<span class="status-ok">{}</span>"#, t.lf_set_configured)
    } else {
        format!(
            r#"<span class="status-missing">{}</span>"#,
            t.lf_set_not_configured
        )
    };

    let encryption_ok = state.config.encryption_key.is_some();
    let encryption_warning = if !encryption_ok {
        format!(
            r#"<div class="alert alert-error">{}</div>"#,
            t.lf_set_encryption_warning
        )
    } else {
        String::new()
    };

    let submit_disabled = if !encryption_ok { " disabled" } else { "" };

    let body = format!(
        r#"<div class="page-header">
            <h1>{title}</h1>
            <p>{subtitle}</p>
        </div>
        {encryption_warning}
        <div class="card" style="max-width: 32rem;">
            <div class="card-body">
                <form method="POST" action="{base}/leanfin/settings" enctype="multipart/form-data">
                    <label for="app_id">{app_id_label}</label>
                    <input type="text" id="app_id" name="app_id" value="{app_id_value}" placeholder="your-app-id">
                    <label>{private_key} — {key_status}</label>
                    <input type="file" id="key_file" name="key_file" accept=".pem,.key">
                    <p class="form-hint">{key_hint}</p>
                    <div style="display:flex; gap:0.75rem; margin-top:1rem;">
                        <a href="{base}/leanfin" class="btn btn-secondary">{cancel}</a>
                        <button type="submit" style="flex:1"{submit_disabled}>{save}</button>
                    </div>
                </form>
            </div>
        </div>"#,
        title = t.lf_set_title,
        subtitle = t.lf_set_subtitle,
        app_id_label = t.lf_set_app_id,
        private_key = t.lf_set_private_key,
        key_hint = t.lf_set_key_hint,
        cancel = t.lf_set_cancel,
        save = t.lf_set_save,
    );

    Html(render_page(
        &format!("LeanFin — {}", t.lf_settings),
        &leanfin_nav(base, "settings", lang),
        &body,
        base,
        lang,
    ))
}

async fn settings_submit(
    state: axum::extract::State<AppState>,
    Extension(user_id): Extension<UserId>,
    Extension(lang): Extension<Lang>,
    mut multipart: Multipart,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let base = &state.config.base_path;
    let t = i18n::t(lang);
    let encryption_key = match state.config.encryption_key.as_deref() {
        Some(k) => k,
        None => {
            return Html("ENCRYPTION_KEY not configured on the server".to_string()).into_response();
        }
    };

    let mut app_id: Option<String> = None;
    let mut key_pem: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "app_id" => {
                app_id = field.text().await.ok().filter(|s| !s.trim().is_empty());
            }
            "key_file" => {
                if let Ok(bytes) = field.bytes().await
                    && !bytes.is_empty()
                {
                    key_pem = Some(bytes.to_vec());
                }
            }
            _ => {}
        }
    }

    // Validate PEM if provided
    if let Some(ref pem_bytes) = key_pem
        && jsonwebtoken::EncodingKey::from_rsa_pem(pem_bytes).is_err()
    {
        let body = format!(
            r#"<div class="page-header">
                    <h1>{title}</h1>
                </div>
                <div class="card" style="max-width: 32rem;">
                    <div class="card-body">
                        <div class="alert alert-error">{invalid_key}</div>
                        <a href="{base}/leanfin/settings" class="btn btn-secondary">{back}</a>
                    </div>
                </div>"#,
            title = t.lf_set_title,
            invalid_key = t.lf_set_invalid_key,
            back = t.lf_set_back,
        );
        return Html(render_page(
            &format!("LeanFin — {}", t.lf_settings),
            &leanfin_nav(base, "settings", lang),
            &body,
            base,
            lang,
        ))
        .into_response();
    }

    // Encrypt key if provided
    let encrypted_key = match key_pem {
        Some(pem_bytes) => match encrypt(&pem_bytes, encryption_key) {
            Ok(enc) => Some(enc),
            Err(e) => {
                tracing::error!("Failed to encrypt key: {e:#}");
                return Html("Failed to encrypt key".to_string()).into_response();
            }
        },
        None => None,
    };

    // UPSERT
    let result = if let Some(enc_key) = encrypted_key {
        sqlx::query(
            r#"INSERT INTO leanfin_user_settings (user_id, enable_banking_app_id, enable_banking_key, updated_at)
               VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
               ON CONFLICT(user_id) DO UPDATE SET
                   enable_banking_app_id = excluded.enable_banking_app_id,
                   enable_banking_key = excluded.enable_banking_key,
                   updated_at = excluded.updated_at"#,
        )
        .bind(user_id.0)
        .bind(&app_id)
        .bind(&enc_key)
        .execute(&state.pool)
        .await
    } else {
        // Only update app_id, keep existing key
        sqlx::query(
            r#"INSERT INTO leanfin_user_settings (user_id, enable_banking_app_id, updated_at)
               VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
               ON CONFLICT(user_id) DO UPDATE SET
                   enable_banking_app_id = excluded.enable_banking_app_id,
                   updated_at = excluded.updated_at"#,
        )
        .bind(user_id.0)
        .bind(&app_id)
        .execute(&state.pool)
        .await
    };

    match result {
        Ok(_) => {
            tracing::info!("Updated Enable Banking settings for user {}", user_id.0);
            axum::response::Redirect::to(&format!("{base}/leanfin/settings")).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to save settings: {e}");
            Html("Failed to save settings".to_string()).into_response()
        }
    }
}
