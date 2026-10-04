//! The egress proxy: the sandbox's only way out.
//!
//! The sandbox runs in its own network namespace with nothing in it but
//! loopback, so it has no route to anywhere. What it does have is a Unix
//! socket, bind-mounted into it by `devbox.sh`, with a `socat` inside relaying
//! `127.0.0.1:3128` to it. `HTTPS_PROXY` points at that port, so every client
//! that honours the proxy environment — cargo, git, npm, gh, Claude Code —
//! reaches the internet through this process, and every client that does not
//! reaches nothing. Failing closed is the point: a tool that ignores the proxy
//! is offline, not unfiltered.
//!
//! Only `CONNECT` is implemented, and the host it names has to be on the
//! allow-list the granted capabilities rendered. There is no TLS interception
//! here and no certificate to install: the tunnel is opaque once established,
//! and what this decides is *who the sandbox may talk to*, not what it may say.
//!
//! The hostname is resolved **here**, on the host, after the check — so a guest
//! cannot pass an allowed name and have it resolve to an address of its
//! choosing. IP literals are refused outright for the same reason: an
//! allow-list of names is only meaningful if names are the only way through.
//! That is also what keeps `169.254.169.254` and the rest of the LAN out
//! without a single deny rule.

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio::time::{Duration, timeout};

/// A request line plus headers past this is not a browser being verbose, it is
/// someone trying to make the parser interesting.
const MAX_HEADER_BYTES: usize = 8 * 1024;

#[derive(Parser)]
#[command(
    name = "devbox-broker-proxy",
    about = "Host-side egress proxy for a devbox sandbox: CONNECT, allow-listed by hostname"
)]
struct Args {
    /// Unix socket to listen on. Not a TCP port: the socket lives in a
    /// directory `devbox.sh` bind-mounts into one sandbox, so *which* sandbox
    /// can reach this proxy is decided by the mount table rather than by a
    /// bearer token. Nothing else on the host is listening for it.
    #[arg(long)]
    listen_unix: PathBuf,

    /// Hosts the sandbox may reach, comma-separated. Exactly the union of what
    /// the granted capabilities asked for; empty means the sandbox reaches
    /// nothing, which is a valid and useful configuration.
    #[arg(long, default_value = "")]
    allow: String,

    /// Ports the sandbox may reach on those hosts.
    #[arg(long, default_value = "443")]
    allow_ports: String,

    /// Sandbox name, recorded in the audit log.
    #[arg(long, default_value = "unknown")]
    sandbox: String,

    #[arg(long)]
    audit_log: Option<PathBuf>,

    #[arg(long, default_value_t = 20)]
    connect_timeout_secs: u64,
}

struct Policy {
    hosts: HashSet<String>,
    ports: HashSet<u16>,
    sandbox: String,
    audit_log: Option<PathBuf>,
    connect_timeout: Duration,
}

/// Why a request was refused, in the words the guest will see.
#[derive(Debug)]
enum Refusal {
    Method(String),
    Malformed,
    IpLiteral,
    Host(String),
    Port(u16),
    Unreachable(String),
}

impl Refusal {
    fn status(&self) -> &'static str {
        match self {
            Refusal::Method(_) => "405 Method Not Allowed",
            Refusal::Malformed => "400 Bad Request",
            Refusal::Unreachable(_) => "502 Bad Gateway",
            _ => "403 Forbidden",
        }
    }

    fn outcome(&self) -> &'static str {
        match self {
            Refusal::Method(_) => "method_refused",
            Refusal::Malformed => "malformed",
            Refusal::IpLiteral => "ip_literal_refused",
            Refusal::Host(_) => "host_refused",
            Refusal::Port(_) => "port_refused",
            Refusal::Unreachable(_) => "unreachable",
        }
    }

    fn message(&self) -> String {
        match self {
            Refusal::Method(method) => format!(
                "this proxy only implements CONNECT, and got {method}. Plain-HTTP \
                 requests are not forwarded: use https."
            ),
            Refusal::Malformed => "could not parse the request line".to_string(),
            Refusal::IpLiteral => {
                "this proxy refuses IP literals — the allow-list is a list of names, \
                 and it resolves them itself"
                    .to_string()
            }
            Refusal::Host(host) => format!(
                "{host} is not in this sandbox's allow-list. Grant a capability that \
                 asks for it on the host: ./devbox.sh capabilities"
            ),
            Refusal::Port(port) => format!("port {port} is not allowed to any host"),
            Refusal::Unreachable(detail) => {
                format!("{detail} — the allow-list permits this host, the network did not")
            }
        }
    }
}

impl Policy {
    /// Exact-match only, deliberately: the fragments name hosts one by one, and
    /// a suffix match would quietly turn `github.com` into every subdomain
    /// anyone can register under it.
    fn check(&self, host: &str, port: u16) -> Result<(), Refusal> {
        if host.parse::<IpAddr>().is_ok() {
            return Err(Refusal::IpLiteral);
        }
        if !self.ports.contains(&port) {
            return Err(Refusal::Port(port));
        }
        if !self.hosts.contains(host) {
            return Err(Refusal::Host(host.to_string()));
        }
        Ok(())
    }

    fn audit(&self, target: &str, outcome: &str, detail: Option<&str>) {
        let Some(path) = &self.audit_log else { return };
        use std::io::Write;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let entry = serde_json::json!({
            "ts": chrono::Utc::now().to_rfc3339(),
            "broker": "proxy",
            "sandbox": self.sandbox,
            "target": target,
            "outcome": outcome,
            "detail": detail,
        });
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{entry}");
        }
    }
}

/// `CONNECT host:port HTTP/1.1`, then headers, then a blank line.
fn parse_connect(line: &str) -> Result<(String, u16), Refusal> {
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or(Refusal::Malformed)?;
    let target = parts.next().ok_or(Refusal::Malformed)?;
    if !method.eq_ignore_ascii_case("CONNECT") {
        return Err(Refusal::Method(method.to_string()));
    }
    // Authority form only: `host:port`, never a URL and never a bare host.
    let (host, port) = target.rsplit_once(':').ok_or(Refusal::Malformed)?;
    let port: u16 = port.parse().map_err(|_| Refusal::Malformed)?;
    let host = host.trim_matches(['[', ']']).trim_end_matches('.');
    if host.is_empty() {
        return Err(Refusal::Malformed);
    }
    Ok((host.to_ascii_lowercase(), port))
}

async fn refuse(stream: &mut UnixStream, refusal: &Refusal) -> Result<()> {
    let body = format!("devbox proxy: {}\n", refusal.message());
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{}",
        refusal.status(),
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Read the request head a byte at a time, stopping exactly at the blank line.
///
/// Not a `BufReader`: that would read ahead past the head and swallow the first
/// bytes of the tunnel. Clients do wait for the `200` before sending, so this
/// has never been observed to matter — but "has never been observed" is not the
/// same as "cannot happen", and the head is at most a few hundred bytes.
async fn read_head(stream: &mut UnixStream) -> Result<Option<String>> {
    let mut head = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte).await? == 0 {
            return Ok(None);
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") || head.ends_with(b"\n\n") {
            break;
        }
        if head.len() >= MAX_HEADER_BYTES {
            break;
        }
    }
    Ok(Some(String::from_utf8_lossy(&head).into_owned()))
}

async fn serve(mut stream: UnixStream, policy: Arc<Policy>) -> Result<()> {
    let Some(head) = read_head(&mut stream).await? else {
        return Ok(());
    };
    let request_line = head.lines().next().unwrap_or_default().trim_end();

    let (host, port) = match parse_connect(request_line) {
        Ok(parsed) => parsed,
        Err(refusal) => {
            policy.audit(request_line, refusal.outcome(), None);
            return refuse(&mut stream, &refusal).await;
        }
    };
    let target = format!("{host}:{port}");

    if let Err(refusal) = policy.check(&host, port) {
        policy.audit(&target, refusal.outcome(), None);
        return refuse(&mut stream, &refusal).await;
    }

    // Resolved here, after the check, so an allowed name cannot be pointed at
    // an address the guest picked.
    let mut upstream = match timeout(policy.connect_timeout, TcpStream::connect(&target)).await {
        Ok(Ok(upstream)) => upstream,
        Ok(Err(err)) => {
            policy.audit(&target, "connect_failed", Some(&err.to_string()));
            let refusal = Refusal::Unreachable(format!("{target}: {err}"));
            return refuse(&mut stream, &refusal).await;
        }
        Err(_) => {
            policy.audit(&target, "connect_timeout", None);
            let refusal = Refusal::Unreachable(format!("{target}: timed out"));
            return refuse(&mut stream, &refusal).await;
        }
    };

    stream
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .await?;
    stream.flush().await?;

    policy.audit(&target, "established", None);
    match tokio::io::copy_bidirectional(&mut stream, &mut upstream).await {
        Ok((up, down)) => policy.audit(&target, "closed", Some(&format!("{up}up/{down}down"))),
        Err(err) => policy.audit(&target, "closed_error", Some(&err.to_string())),
    }
    Ok(())
}

fn parse_list<T, F>(raw: &str, parse: F) -> Vec<T>
where
    F: Fn(&str) -> Option<T>,
{
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(parse)
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let hosts: HashSet<String> = parse_list(&args.allow, |host| {
        Some(host.to_ascii_lowercase().trim_end_matches('.').to_string())
    })
    .into_iter()
    .collect();
    let ports: HashSet<u16> = parse_list(&args.allow_ports, |port| port.parse().ok())
        .into_iter()
        .collect();

    // A socket left behind by a crash would otherwise make every later start
    // fail with EADDRINUSE on a path nothing is listening on.
    let _ = std::fs::remove_file(&args.listen_unix);
    if let Some(parent) = args.listen_unix.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let listener = UnixListener::bind(&args.listen_unix)
        .with_context(|| format!("binding {}", args.listen_unix.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&args.listen_unix, std::fs::Permissions::from_mode(0o600))?;
    }

    eprintln!(
        "proxy: listening on {} — {} host(s) allowed on port(s) {}",
        args.listen_unix.display(),
        hosts.len(),
        args.allow_ports
    );

    let policy = Arc::new(Policy {
        hosts,
        ports,
        sandbox: args.sandbox,
        audit_log: args.audit_log,
        connect_timeout: Duration::from_secs(args.connect_timeout_secs),
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted.context("accepting")?;
                let policy = Arc::clone(&policy);
                tokio::spawn(async move {
                    if let Err(err) = serve(stream, policy).await {
                        eprintln!("proxy: connection ended: {err}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
        }
    }

    let _ = std::fs::remove_file(&args.listen_unix);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(hosts: &[&str]) -> Policy {
        Policy {
            hosts: hosts.iter().map(|h| h.to_string()).collect(),
            ports: [443].into_iter().collect(),
            sandbox: "test".into(),
            audit_log: None,
            connect_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn the_authority_form_is_what_connect_carries() {
        assert_eq!(
            parse_connect("CONNECT crates.io:443 HTTP/1.1").unwrap(),
            ("crates.io".to_string(), 443)
        );
        // Case and a trailing root dot are the same host.
        assert_eq!(
            parse_connect("connect GitHub.com.:443 HTTP/1.1").unwrap(),
            ("github.com".to_string(), 443)
        );
    }

    #[test]
    fn anything_that_is_not_a_connect_is_refused_rather_than_forwarded() {
        for line in [
            "GET http://crates.io/ HTTP/1.1",
            "POST https://api.github.com HTTP/1.1",
        ] {
            assert!(matches!(
                parse_connect(line),
                Err(Refusal::Method(_) | Refusal::Malformed)
            ));
        }
        for line in ["CONNECT crates.io HTTP/1.1", "CONNECT :443 HTTP/1.1", ""] {
            assert!(matches!(parse_connect(line), Err(Refusal::Malformed)));
        }
    }

    #[test]
    fn only_the_listed_hosts_get_through() {
        let policy = policy(&["crates.io", "static.crates.io"]);
        assert!(policy.check("crates.io", 443).is_ok());
        assert!(matches!(
            policy.check("evil.example", 443),
            Err(Refusal::Host(_))
        ));
        // Not a suffix match: a subdomain anyone can register is not the host.
        assert!(matches!(
            policy.check("crates.io.evil.example", 443),
            Err(Refusal::Host(_))
        ));
        assert!(matches!(
            policy.check("crates.io", 80),
            Err(Refusal::Port(_))
        ));
    }

    #[test]
    fn an_ip_literal_never_matches_an_allow_list_of_names() {
        // The link-local metadata address is the classic target, and it needs
        // no deny rule: it is not a name, so it cannot be on the list.
        let policy = policy(&["crates.io"]);
        for literal in ["169.254.169.254", "127.0.0.1", "::1", "10.0.0.5"] {
            assert!(matches!(
                policy.check(literal, 443),
                Err(Refusal::IpLiteral)
            ));
        }
    }
}
