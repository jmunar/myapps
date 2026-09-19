//! The only place the Anthropic credential is read.
//!
//! Default behaviour is to *follow* `~/.claude/.credentials.json` rather than
//! refresh it. Claude Code on the host already refreshes that file as you use
//! it, and OAuth refresh tokens rotate: a second refresh chain would invalidate
//! the host's login. Refreshing is therefore opt-in, and when it is on the new
//! token is written back to the same file so there is still only one chain.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefreshMode {
    /// Never refresh. An expired token is a 401 telling you to run `claude`
    /// on the host, which refreshes the file the broker follows.
    Off,
    /// Refresh and write the result back to the credentials file.
    Writeback,
}

impl std::str::FromStr for RefreshMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "writeback" => Ok(Self::Writeback),
            other => Err(format!("unknown refresh mode '{other}' (off|writeback)")),
        }
    }
}

pub enum Source {
    /// The subscription OAuth token, followed from Claude Code's own file.
    OauthFile {
        path: PathBuf,
        mode: RefreshMode,
        client_id: Option<String>,
        token_url: String,
    },
    /// A plain API key. The supported path, and the fallback if OAuth
    /// injection ever stops working.
    ApiKey(String),
}

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<Oauth>,
}

#[derive(Deserialize, Clone)]
struct Oauth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    /// Milliseconds since the epoch.
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// What the broker puts on the wire, and what it tells you about the token.
pub struct Credential {
    pub header: (&'static str, String),
    pub expires_in_secs: Option<i64>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_oauth(path: &Path) -> Result<Oauth> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: CredentialsFile =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    parsed
        .oauth
        .ok_or_else(|| anyhow::anyhow!("{} has no claudeAiOauth block", path.display()))
}

impl Source {
    /// Re-read on every request. It is one small file, and reading it is what
    /// makes a refresh performed by the host's own Claude Code visible here
    /// without restarting the broker.
    pub async fn current(&self) -> Result<Credential> {
        match self {
            Source::ApiKey(key) => Ok(Credential {
                header: ("x-api-key", key.clone()),
                expires_in_secs: None,
            }),
            Source::OauthFile {
                path,
                mode,
                client_id,
                token_url,
            } => {
                let oauth = read_oauth(path)?;
                let remaining = oauth.expires_at.map(|at| (at - now_ms()) / 1000);

                // 60s of slack: a token that dies mid-request is a failed turn.
                let stale = remaining.is_some_and(|secs| secs < 60);
                if !stale {
                    return Ok(Credential {
                        header: ("authorization", format!("Bearer {}", oauth.access_token)),
                        expires_in_secs: remaining,
                    });
                }

                match mode {
                    RefreshMode::Off => bail!(
                        "the OAuth token in {} expired. Run `claude` on the host to refresh it, \
                         or start the broker with --refresh writeback",
                        path.display()
                    ),
                    RefreshMode::Writeback => {
                        let refreshed = refresh(&oauth, client_id.as_deref(), token_url).await?;
                        write_back(path, &refreshed)?;
                        Ok(Credential {
                            header: (
                                "authorization",
                                format!("Bearer {}", refreshed.access_token),
                            ),
                            expires_in_secs: refreshed.expires_at.map(|at| (at - now_ms()) / 1000),
                        })
                    }
                }
            }
        }
    }
}

async fn refresh(oauth: &Oauth, client_id: Option<&str>, token_url: &str) -> Result<Oauth> {
    let refresh_token = oauth
        .refresh_token
        .as_deref()
        .context("the credentials file has no refreshToken")?;
    let client_id = client_id.context(
        "--oauth-client-id is required with --refresh writeback \
         (the public client id Claude Code itself uses)",
    )?;

    let response = reqwest::Client::new()
        .post(token_url)
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": client_id,
        }))
        .send()
        .await
        .context("refreshing the OAuth token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("token refresh failed with {status}: {body}");
    }

    let body: RefreshResponse = response
        .json()
        .await
        .context("parsing the refresh response")?;
    Ok(Oauth {
        access_token: body.access_token,
        // A rotating refresh token that is not stored is a login thrown away.
        refresh_token: body.refresh_token.or_else(|| oauth.refresh_token.clone()),
        expires_at: body.expires_in.map(|secs| now_ms() + secs * 1000),
    })
}

/// Rewrite only the three fields we own, preserving everything else in the
/// file, and swap it into place atomically so a crash cannot leave Claude Code
/// on the host without a login.
fn write_back(path: &Path, oauth: &Oauth) -> Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let mut doc: serde_json::Value = serde_json::from_str(&raw)?;
    let block = doc
        .get_mut("claudeAiOauth")
        .context("no claudeAiOauth block to update")?;
    block["accessToken"] = serde_json::json!(oauth.access_token);
    if let Some(token) = &oauth.refresh_token {
        block["refreshToken"] = serde_json::json!(token);
    }
    if let Some(at) = oauth.expires_at {
        block["expiresAt"] = serde_json::json!(at);
    }

    let tmp = path.with_extension("broker.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&doc)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}
