//! The Anthropic credential broker.
//!
//! One process per sandbox, listening on one Unix socket. The socket path *is*
//! the sandbox's identity: there is no bearer token to manage and nothing on
//! the host or the LAN can reach it, because it is not on a network at all —
//! `msb --vsock` routes it to a guest port, and a shim in the guest bridges
//! that to the loopback address Claude Code is pointed at.
//!
//! What the guest sends as `Authorization` is discarded before the request
//! leaves the host. It never learns the real credential, only that requests
//! work.

mod credentials;

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use clap::Parser;
use credentials::{RefreshMode, Source};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Headers that belong to one hop and must not be forwarded, plus every way a
/// caller could try to supply its own credential.
const STRIPPED: &[&str] = &[
    "authorization",
    "x-api-key",
    "cookie",
    "host",
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "content-length",
];

#[derive(Parser)]
#[command(
    name = "msb-broker-anthropic",
    about = "Host-side Anthropic credential broker"
)]
struct Args {
    /// Unix socket to listen on. One per sandbox.
    #[arg(long)]
    socket: PathBuf,

    /// Sandbox name, recorded in the audit log.
    #[arg(long, default_value = "unknown")]
    sandbox: String,

    /// Credentials file to follow.
    #[arg(
        long,
        env = "CLAUDE_CREDENTIALS",
        default_value = "~/.claude/.credentials.json"
    )]
    credentials: String,

    /// Use an API key instead of the subscription token.
    #[arg(long, env = "ANTHROPIC_API_KEY")]
    api_key: Option<String>,

    /// `off` follows the file and 401s when it expires; `writeback` refreshes
    /// and rewrites the file.
    #[arg(long, default_value = "off")]
    refresh: RefreshMode,

    /// Required by `--refresh writeback`.
    #[arg(long, env = "CLAUDE_OAUTH_CLIENT_ID")]
    oauth_client_id: Option<String>,

    #[arg(long, default_value = "https://console.anthropic.com/v1/oauth/token")]
    oauth_token_url: String,

    #[arg(long, default_value = "https://api.anthropic.com")]
    upstream: String,

    /// Path prefixes the guest may reach. Anything else is refused here.
    /// `/api/hello` is Claude Code's token-validation probe.
    #[arg(long, default_value = "/v1/messages,/v1/models,/api/hello")]
    allow_paths: String,

    /// Beta header the OAuth path needs, merged into whatever the client sent.
    /// Empty disables it.
    #[arg(long, default_value = "oauth-2025-04-20")]
    oauth_beta: String,

    #[arg(long)]
    audit_log: Option<PathBuf>,

    /// Refuse past this many requests in a rolling hour. Unset means no limit.
    #[arg(long)]
    max_requests_per_hour: Option<usize>,

    #[arg(long, default_value_t = 64)]
    max_body_mb: usize,
}

struct AppState {
    source: Source,
    client: reqwest::Client,
    upstream: String,
    allow_paths: Vec<String>,
    oauth_beta: Option<String>,
    sandbox: String,
    audit_log: Option<PathBuf>,
    max_body: usize,
    max_per_hour: Option<usize>,
    recent: Mutex<Vec<Instant>>,
}

fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    }
}

fn error(status: StatusCode, message: &str) -> Response {
    // Shaped like an Anthropic error so the client surfaces the text rather
    // than "unexpected response".
    let body = serde_json::json!({
        "type": "error",
        "error": { "type": "broker_error", "message": message },
    });
    (status, axum::Json(body)).into_response()
}

impl AppState {
    fn over_limit(&self) -> bool {
        let Some(max) = self.max_per_hour else {
            return false;
        };
        let mut recent = self.recent.lock().expect("lock poisoned");
        let hour_ago = Instant::now() - std::time::Duration::from_secs(3600);
        recent.retain(|at| *at > hour_ago);
        if recent.len() >= max {
            return true;
        }
        recent.push(Instant::now());
        false
    }

    fn audit(&self, entry: serde_json::Value) {
        let Some(path) = &self.audit_log else { return };
        use std::io::Write;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    match state.source.current().await {
        Ok(credential) => axum::Json(serde_json::json!({
            "status": "ok",
            "sandbox": state.sandbox,
            "auth": credential.header.0,
            "expires_in_secs": credential.expires_in_secs,
        }))
        .into_response(),
        Err(err) => error(StatusCode::SERVICE_UNAVAILABLE, &err.to_string()),
    }
}

async fn proxy(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let started = Instant::now();
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let query = parts
        .uri
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    if !state
        .allow_paths
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        state.audit(serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "sandbox": state.sandbox, "path": path, "outcome": "path_refused",
        }));
        return error(
            StatusCode::FORBIDDEN,
            &format!("the broker does not forward {path} (see --allow-paths)"),
        );
    }

    if state.over_limit() {
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "sandbox request budget exhausted",
        );
    }

    let bytes = match axum::body::to_bytes(body, state.max_body).await {
        Ok(bytes) => bytes,
        Err(_) => return error(StatusCode::PAYLOAD_TOO_LARGE, "request body too large"),
    };

    // Only to label the audit line; a body that is not JSON still forwards.
    let (model, streaming) = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map(|v| {
            (
                v.get("model").and_then(|m| m.as_str()).map(str::to_string),
                v.get("stream").and_then(|s| s.as_bool()).unwrap_or(false),
            )
        })
        .unwrap_or((None, false));

    let credential = match state.source.current().await {
        Ok(credential) => credential,
        Err(err) => return error(StatusCode::UNAUTHORIZED, &err.to_string()),
    };

    let mut headers = HeaderMap::new();
    for (name, value) in parts.headers.iter() {
        if STRIPPED.contains(&name.as_str()) {
            continue;
        }
        headers.insert(name.clone(), value.clone());
    }
    match HeaderValue::from_str(&credential.header.1) {
        Ok(value) => {
            headers.insert(HeaderName::from_static(credential.header.0), value);
        }
        Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "malformed credential"),
    }
    if let Some(beta) = &state.oauth_beta
        && credential.header.0 == "authorization"
    {
        let merged = match headers.get("anthropic-beta").and_then(|v| v.to_str().ok()) {
            Some(existing) if existing.split(',').any(|part| part.trim() == beta) => {
                existing.to_string()
            }
            Some(existing) => format!("{existing},{beta}"),
            None => beta.clone(),
        };
        if let Ok(value) = HeaderValue::from_str(&merged) {
            headers.insert("anthropic-beta", value);
        }
    }

    let url = format!("{}{}{}", state.upstream, path, query);
    let upstream = state
        .client
        .request(parts.method.clone(), &url)
        .headers(headers)
        .body(bytes.clone())
        .send()
        .await;

    let response = match upstream {
        Ok(response) => response,
        Err(err) => {
            state.audit(serde_json::json!({
                "ts": chrono::Utc::now().to_rfc3339(), "sandbox": state.sandbox,
                "path": path, "model": model, "outcome": "upstream_error",
                "error": err.to_string(),
            }));
            return error(StatusCode::BAD_GATEWAY, &format!("upstream: {err}"));
        }
    };

    let status = response.status();
    state.audit(serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "sandbox": state.sandbox,
        "method": parts.method.as_str(),
        "path": path,
        "model": model,
        "stream": streaming,
        "status": status.as_u16(),
        "request_bytes": bytes.len(),
        "latency_ms": started.elapsed().as_millis() as u64,
    }));

    let mut builder = Response::builder().status(status);
    for (name, value) in response.headers().iter() {
        if STRIPPED.contains(&name.as_str()) {
            continue;
        }
        builder = builder.header(name.clone(), value.clone());
    }
    // Streamed straight through: buffering here would hold back every SSE
    // event until the turn ended.
    builder
        .body(Body::from_stream(response.bytes_stream()))
        .unwrap_or_else(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "malformed upstream response",
            )
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let source = match args.api_key {
        Some(key) => Source::ApiKey(key),
        None => Source::OauthFile {
            path: expand_tilde(&args.credentials),
            mode: args.refresh,
            client_id: args.oauth_client_id,
            token_url: args.oauth_token_url,
        },
    };

    if args.refresh == RefreshMode::Writeback {
        eprintln!(
            "broker: refresh=writeback rewrites {} — the host's own Claude Code \
             login shares that file",
            args.credentials
        );
    }

    let state = Arc::new(AppState {
        source,
        client: reqwest::Client::builder()
            .user_agent(concat!("msb-broker-anthropic/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building the upstream client")?,
        upstream: args.upstream.trim_end_matches('/').to_string(),
        allow_paths: args.allow_paths.split(',').map(str::to_string).collect(),
        oauth_beta: Some(args.oauth_beta).filter(|b| !b.is_empty()),
        sandbox: args.sandbox,
        audit_log: args.audit_log,
        max_body: args.max_body_mb * 1024 * 1024,
        max_per_hour: args.max_requests_per_hour,
        recent: Mutex::new(Vec::new()),
    });

    if let Some(parent) = args.socket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    // A socket left behind by a killed broker would otherwise block the bind.
    let _ = std::fs::remove_file(&args.socket);
    let listener = tokio::net::UnixListener::bind(&args.socket)
        .with_context(|| format!("binding {}", args.socket.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&args.socket, std::fs::Permissions::from_mode(0o600))?;
    }

    let app = Router::new()
        .route("/_broker/health", get(health))
        .fallback(proxy)
        .with_state(state);

    eprintln!("broker: listening on {}", args.socket.display());

    let socket = args.socket.clone();
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    let _ = std::fs::remove_file(&socket);
    result.context("serving")
}
