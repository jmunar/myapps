//! The prod broker: read-only access to the real deployment, for a sandbox.
//!
//! Same shape as `brokers/anthropic` — one process per sandbox, one host
//! loopback port, answering only requests that carry that sandbox's bearer
//! token — and the same reason for existing: the credential stays on the host.
//! Here it is the Odroid's SSH key, which `sandbox/CLAUDE.md` says permanently
//! never enters a VM.
//!
//! What makes this safe is not filtering but *shape*. There is no endpoint
//! that takes a command, a path or a query. The guest picks a verb — snapshot,
//! logs, status — and fills parameters into templates the broker owns, each
//! one validated against a closed set first. A tunnel with an allow-list in
//! front of it would be a different thing entirely, and `deploy.sh` would
//! still be one `ssh` away.
//!
//! The database never crosses the boundary as it is. `sqlite3 .dump` streams
//! it to the host, the host rebuilds it, scrubs it — see `scrub.rs`, which
//! refuses outright if prod holds a table nobody has classified — and only the
//! scrubbed copy is served.
//!
//! Every verb is a read. Nothing here writes to prod, and there is no code
//! path that could: the three command strings in `remote.rs` are the entire
//! vocabulary.

mod remote;
mod scrub;

use anyhow::{Context, Result, bail};
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use clap::Parser;
use remote::{DeployEnv, Remote};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "msb-broker-prod",
    about = "Host-side prod broker: read-only snapshots and logs, behind a verb-limited API"
)]
struct Args {
    /// Loopback only, like the Anthropic broker: msb's `host` network group
    /// reaches a host loopback port from inside a guest, so binding wider
    /// would only widen who can ask.
    #[arg(long, default_value = "127.0.0.1:8788")]
    listen: SocketAddr,

    /// File holding the bearer token the sandbox must present.
    #[arg(long)]
    token_file: PathBuf,

    /// Sandbox name, recorded in the audit log.
    #[arg(long, default_value = "unknown")]
    sandbox: String,

    /// Which deployment to read. The same file `deploy.sh` reads, so prod is
    /// described in one place; point it at deploy/stage.env for staging.
    #[arg(long, env = "DEVBOX_PROD_ENV", default_value = "deploy/prod.env")]
    deploy_env: PathBuf,

    /// SSH key for the Odroid. Unset uses the host's normal SSH configuration
    /// (agent, ~/.ssh/config), which is the usual case — the point is that
    /// whichever it is, it stays here.
    #[arg(long)]
    ssh_key: Option<PathBuf>,

    /// The service account that owns the database on the Odroid.
    #[arg(long, default_value = "myapps")]
    service_user: String,

    /// Where dumps are rebuilt and scrubbed. Defaults under the same cache
    /// directory the sandboxes use, so `devbox.sh remove` can reclaim it.
    #[arg(long)]
    work_dir: Option<PathBuf>,

    /// Units `?unit=` may name. Defaults to the deployment's own service plus
    /// nginx; anything else is refused before a command is built.
    #[arg(long)]
    allow_units: Option<String>,

    /// Password every account gets in a scrubbed snapshot.
    #[arg(long, default_value = "dev")]
    login_password: String,

    /// Refuse a second snapshot within this many seconds. A snapshot is a full
    /// dump of a 4 GB machine's database over the LAN; an agent in a retry
    /// loop should not be able to spend the Odroid's afternoon on it.
    #[arg(long, default_value_t = 60)]
    min_snapshot_interval_secs: u64,

    /// Abandon a dump past this size rather than filling the host's disk.
    #[arg(long, default_value_t = 512)]
    max_dump_mb: u64,

    /// Ceiling on `?lines=`.
    #[arg(long, default_value_t = 2000)]
    max_log_lines: u32,

    /// How long any one remote command may take.
    #[arg(long, default_value_t = 300)]
    ssh_timeout_secs: u64,

    #[arg(long)]
    audit_log: Option<PathBuf>,
}

struct AppState {
    token: String,
    remote: Remote,
    target: String,
    work_dir: PathBuf,
    allow_units: Vec<String>,
    password: String,
    sandbox: String,
    audit_log: Option<PathBuf>,
    max_dump_bytes: u64,
    max_log_lines: u32,
    min_snapshot_interval: Duration,
    last_snapshot: Mutex<Option<Instant>>,
    /// One dump at a time. Two concurrent snapshots would race on the same
    /// working files and ask the Odroid to do the work twice.
    snapshot_lock: tokio::sync::Mutex<()>,
}

/// Length is not secret — the token is fixed-width hex — but the comparison
/// runs to the end anyway so a wrong guess leaks nothing by timing.
fn secret_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn read_token(path: &Path) -> Result<String> {
    let token = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?
        .trim()
        .to_string();
    if token.is_empty() {
        bail!(
            "{} is empty — the broker will not run without a token",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            eprintln!(
                "broker: warning: {} is readable beyond its owner (mode {:o})",
                path.display(),
                mode & 0o777
            );
        }
    }
    Ok(token)
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

impl AppState {
    fn authorized(&self, headers: &HeaderMap) -> bool {
        // No unauthenticated exception here, unlike the Anthropic broker: that
        // one forwards Claude Code's pre-token `/api/hello` probe, and nothing
        // reaches this broker before its client has the token.
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
            .is_some_and(|token| secret_eq(token, &self.token))
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

    fn record(&self, verb: &str, outcome: &str, detail: serde_json::Value) {
        self.audit(serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "broker": "prod",
            "sandbox": self.sandbox,
            "target": self.target,
            "verb": verb,
            "outcome": outcome,
            "detail": detail,
        }));
    }

    /// `?unit=` against the closed set. Defaults to the deployment's service.
    fn unit(&self, params: &HashMap<String, String>) -> Result<String, Box<Response>> {
        let unit = params
            .get("unit")
            .map(String::as_str)
            .unwrap_or_else(|| self.remote.service());
        if !self.allow_units.iter().any(|allowed| allowed == unit) {
            return Err(Box::new(error(
                StatusCode::FORBIDDEN,
                &format!(
                    "unit '{unit}' is not one this broker will read (allowed: {})",
                    self.allow_units.join(", ")
                ),
            )));
        }
        Ok(unit.to_string())
    }
}

/// `--since` for journalctl, from a closed grammar rather than passed through.
///
/// `5m` and `2h` are what anyone types; journalctl wants them signed, so they
/// are rewritten rather than rejected. An absolute date is allowed because
/// "since the deploy" is the other real question. Everything else — including
/// journalctl's own English forms, which are a parser all of their own — is
/// refused.
fn parse_since(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty --since".into());
    }

    let relative = raw.strip_prefix('-').unwrap_or(raw);
    if let Some(unit) = relative.chars().last()
        && matches!(unit, 's' | 'm' | 'h' | 'd')
        && relative.len() > 1
        && relative[..relative.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit())
    {
        return Ok(format!("-{relative}"));
    }

    // YYYY-MM-DD[ HH:MM[:SS]]
    let (date, time) = match raw.split_once(' ') {
        Some((date, time)) => (date, Some(time)),
        None => (raw, None),
    };
    let date_ok = date.len() == 10
        && date.as_bytes()[4] == b'-'
        && date.as_bytes()[7] == b'-'
        && date
            .char_indices()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit());
    let time_ok = match time {
        None => true,
        Some(time) => {
            matches!(time.len(), 5 | 8)
                && time
                    .char_indices()
                    .all(|(i, c)| matches!(i, 2 | 5) == !c.is_ascii_digit())
        }
    };
    if date_ok && time_ok {
        return Ok(raw.to_string());
    }

    Err(format!(
        "'{raw}' is not a window this broker understands: use 30s, 15m, 2h, 7d \
         or YYYY-MM-DD[ HH:MM[:SS]]"
    ))
}

async fn sqlite(db: &Path, script: Option<&Path>, query: Option<&str>) -> Result<String> {
    let mut command = tokio::process::Command::new("sqlite3");
    // -bail: without it sqlite3 reports an error on one statement and cheerfully
    // carries on with the rest, which for a scrub means a half-scrubbed
    // database served as if it were clean.
    command.arg("-bail").arg(db);
    if let Some(query) = query {
        command.arg(query);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match script {
        Some(path) => {
            let file =
                std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
            command.stdin(Stdio::from(file));
        }
        None => {
            command.stdin(Stdio::null());
        }
    }

    let output = command
        .spawn()
        .context("spawning sqlite3 — is it installed on the host?")?
        .wait_with_output()
        .await
        .context("waiting for sqlite3")?;
    if !output.status.success() {
        bail!(
            "sqlite3 failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn health(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return unauthorized(&state, "health");
    }
    // Deliberately cheap and deliberately quiet about where prod is: the
    // sandbox learns which deployment it is talking to and which units it may
    // read, never the host it reaches. `devbox.sh up` calls this on every
    // start, so it must not depend on the LAN either.
    axum::Json(serde_json::json!({
        "status": "ok",
        "sandbox": state.sandbox,
        "target": state.target,
        "service": state.remote.service(),
        "units": state.allow_units,
        "snapshot_password": state.password,
        "verbs": ["/snapshot/db", "/logs", "/status"],
    }))
    .into_response()
}

fn unauthorized(state: &AppState, verb: &str) -> Response {
    state.record(verb, "unauthorized", serde_json::Value::Null);
    error(
        StatusCode::UNAUTHORIZED,
        "this broker belongs to another sandbox (bad or missing bearer token)",
    )
}

async fn snapshot(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !state.authorized(&headers) {
        return unauthorized(&state, "snapshot");
    }

    {
        let last = state.last_snapshot.lock().expect("lock poisoned");
        if let Some(at) = *last
            && at.elapsed() < state.min_snapshot_interval
        {
            let wait = (state.min_snapshot_interval - at.elapsed()).as_secs() + 1;
            state.record(
                "snapshot",
                "throttled",
                serde_json::json!({ "wait_s": wait }),
            );
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                &format!("a snapshot was taken recently — try again in {wait}s"),
            );
        }
    }

    let _one_at_a_time = state.snapshot_lock.lock().await;
    let started = Instant::now();

    match build_snapshot(&state).await {
        Ok((path, bytes)) => {
            *state.last_snapshot.lock().expect("lock poisoned") = Some(Instant::now());
            state.record(
                "snapshot",
                "ok",
                serde_json::json!({
                    "dump_bytes": bytes,
                    "seconds": started.elapsed().as_secs(),
                }),
            );
            match tokio::fs::File::open(&path).await {
                Ok(file) => {
                    // Streamed off disk: a snapshot is the whole database, and
                    // collecting it into a response body would hold a second
                    // copy of it in the broker.
                    let stream = tokio_util::io::ReaderStream::new(file);
                    let mut response = Body::from_stream(stream).into_response();
                    let out = response.headers_mut();
                    out.insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/vnd.sqlite3"),
                    );
                    out.insert(
                        header::CONTENT_DISPOSITION,
                        HeaderValue::from_static("attachment; filename=\"myapps-prod.db\""),
                    );
                    // So the caller can say how to log in without a second
                    // request. Configuration, not a secret.
                    if let Ok(value) = HeaderValue::from_str(&state.password) {
                        out.insert(HeaderName::from_static("x-snapshot-password"), value);
                    }
                    response
                }
                Err(err) => error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("opening the finished snapshot: {err}"),
                ),
            }
        }
        Err(err) => {
            state.record(
                "snapshot",
                "failed",
                serde_json::json!({ "error": err.to_string() }),
            );
            error(StatusCode::BAD_GATEWAY, &format!("{err:#}"))
        }
    }
}

/// Dump, rebuild, scrub. The scrubbed database is a different file from the
/// rebuilt one only in name — the rebuild is scrubbed in place and then
/// vacuumed, so the pages the deleted rows occupied are gone rather than
/// merely unreferenced.
async fn build_snapshot(state: &AppState) -> Result<(PathBuf, u64)> {
    tokio::fs::create_dir_all(&state.work_dir)
        .await
        .with_context(|| format!("creating {}", state.work_dir.display()))?;

    let dump = state.work_dir.join("prod.sql");
    let db = state.work_dir.join("snapshot.db");
    let script = state.work_dir.join("scrub.sql");

    let bytes = state
        .remote
        .dump_database(&dump, state.max_dump_bytes)
        .await?;

    // Rebuilt from empty every time: sqlite3 would otherwise apply the dump on
    // top of the previous snapshot and fail on the first duplicate key.
    let _ = tokio::fs::remove_file(&db).await;
    sqlite(&db, Some(&dump), None)
        .await
        .context("rebuilding the dump into a database")?;
    // The dump is the unscrubbed copy. It has served its purpose.
    let _ = tokio::fs::remove_file(&dump).await;

    let listed = sqlite(
        &db,
        None,
        Some("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name"),
    )
    .await
    .context("listing the snapshot's tables")?;
    let tables: Vec<String> = listed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    let password_hash = hash_password(&state.password)?;
    // Refuses here, before anything is served, if prod holds a table this
    // broker has never been told about.
    let sql = scrub::script(&tables, &password_hash)?;
    tokio::fs::write(&script, &sql)
        .await
        .context("writing the scrub script")?;
    sqlite(&db, Some(&script), None)
        .await
        .context("scrubbing the snapshot")?;
    let _ = tokio::fs::remove_file(&script).await;

    Ok((db, bytes))
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut rand_core_06::OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing the snapshot password: {e}"))?
        .to_string())
}

async fn logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !state.authorized(&headers) {
        return unauthorized(&state, "logs");
    }
    let unit = match state.unit(&params) {
        Ok(unit) => unit,
        Err(response) => return *response,
    };
    let since = match parse_since(params.get("since").map(String::as_str).unwrap_or("1h")) {
        Ok(since) => since,
        Err(message) => return error(StatusCode::BAD_REQUEST, &message),
    };
    let lines = match params.get("lines").map(|value| value.parse::<u32>()) {
        None => 200,
        Some(Ok(lines)) if lines > 0 && lines <= state.max_log_lines => lines,
        Some(_) => {
            return error(
                StatusCode::BAD_REQUEST,
                &format!("lines must be between 1 and {}", state.max_log_lines),
            );
        }
    };

    match state.remote.logs(&unit, &since, lines).await {
        Ok(output) if output.ok => {
            state.record(
                "logs",
                "ok",
                serde_json::json!({ "unit": unit, "since": since, "lines": lines }),
            );
            (
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                output.stdout,
            )
                .into_response()
        }
        Ok(output) => {
            state.record("logs", "refused", serde_json::json!({ "unit": unit }));
            error(StatusCode::BAD_GATEWAY, &format!("prod: {}", output.stderr))
        }
        Err(err) => {
            state.record(
                "logs",
                "failed",
                serde_json::json!({ "error": err.to_string() }),
            );
            error(StatusCode::BAD_GATEWAY, &format!("{err:#}"))
        }
    }
}

async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !state.authorized(&headers) {
        return unauthorized(&state, "status");
    }
    let unit = match state.unit(&params) {
        Ok(unit) => unit,
        Err(response) => return *response,
    };
    match state.remote.status(&unit).await {
        // `systemctl status` exits non-zero for a stopped unit, which is an
        // answer rather than a failure — forward whatever it printed.
        Ok(output) if !output.stdout.is_empty() => {
            state.record("status", "ok", serde_json::json!({ "unit": unit }));
            (
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                output.stdout,
            )
                .into_response()
        }
        Ok(output) => {
            state.record("status", "refused", serde_json::json!({ "unit": unit }));
            error(StatusCode::BAD_GATEWAY, &format!("prod: {}", output.stderr))
        }
        Err(err) => {
            state.record(
                "status",
                "failed",
                serde_json::json!({ "error": err.to_string() }),
            );
            error(StatusCode::BAD_GATEWAY, &format!("{err:#}"))
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.listen.ip().is_loopback() {
        bail!(
            "refusing to listen on {} — the broker binds loopback only; the guest \
             reaches it through msb's `host` network group",
            args.listen
        );
    }

    let env = DeployEnv::read(&args.deploy_env)?;
    let target = args
        .deploy_env
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "prod".into());
    let service = env.service.clone();
    let remote = Remote::new(
        env,
        args.ssh_key,
        args.service_user,
        Duration::from_secs(args.ssh_timeout_secs),
    );

    let allow_units: Vec<String> = match &args.allow_units {
        Some(list) => list
            .split(',')
            .map(str::trim)
            .filter(|unit| !unit.is_empty())
            .map(str::to_string)
            .collect(),
        None => vec![service.clone(), "nginx".to_string()],
    };

    let work_dir = args.work_dir.unwrap_or_else(|| {
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir);
        cache.join("msb-devbox/prod").join(&args.sandbox)
    });

    let token = read_token(&args.token_file)?;

    let state = Arc::new(AppState {
        token,
        remote,
        target,
        work_dir,
        allow_units,
        password: args.login_password,
        sandbox: args.sandbox,
        audit_log: args.audit_log,
        max_dump_bytes: args.max_dump_mb * 1024 * 1024,
        max_log_lines: args.max_log_lines,
        min_snapshot_interval: Duration::from_secs(args.min_snapshot_interval_secs),
        last_snapshot: Mutex::new(None),
        snapshot_lock: tokio::sync::Mutex::new(()),
    });

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;

    // No fallback route, unlike the Anthropic broker: that one is a proxy and
    // forwards what it does not recognise. This one has a vocabulary, and
    // anything outside it is a 404 rather than a question for prod.
    let app = Router::new()
        .route("/_broker/health", get(health))
        .route("/snapshot/db", get(snapshot))
        .route("/logs", get(logs))
        .route("/status", get(status))
        .with_state(state);

    eprintln!("broker: prod broker listening on {}", args.listen);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("serving")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_windows_are_signed_for_journalctl() {
        assert_eq!(parse_since("2h").unwrap(), "-2h");
        assert_eq!(parse_since("30s").unwrap(), "-30s");
        assert_eq!(parse_since("-15m").unwrap(), "-15m");
        assert_eq!(parse_since("7d").unwrap(), "-7d");
    }

    #[test]
    fn absolute_dates_pass_through() {
        assert_eq!(parse_since("2026-09-20").unwrap(), "2026-09-20");
        assert_eq!(parse_since("2026-09-20 14:30").unwrap(), "2026-09-20 14:30");
        assert_eq!(
            parse_since("2026-09-20 14:30:05").unwrap(),
            "2026-09-20 14:30:05"
        );
    }

    #[test]
    fn anything_that_is_not_a_window_is_refused() {
        for bad in [
            "yesterday",
            "1 hour ago",
            "2h; rm -rf /",
            "$(whoami)",
            "",
            "h",
            "2026-9-20",
            "2026-09-20 14",
        ] {
            assert!(parse_since(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn the_token_comparison_rejects_a_different_length() {
        assert!(secret_eq("abc", "abc"));
        assert!(!secret_eq("abc", "abcd"));
        assert!(!secret_eq("abc", "abd"));
    }
}
