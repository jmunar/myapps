//! The Odroid, reached by `ssh`, and the only commands that may be run on it.
//!
//! Nothing the guest sends becomes part of a command. Each method here builds
//! one fixed command string and fills in values the broker itself owns or has
//! already validated against a closed set — that is what makes the API
//! verb-limited rather than a tunnel with a filter in front of it. The
//! shell-quoting below is a second line, not the first one.
//!
//! All three commands are reads. `.dump` runs in a read transaction and writes
//! nothing, not even a temporary file, which is why it is preferred here over
//! the `.backup` the manual backup procedure uses: there is no remote artifact
//! to clean up afterwards, and nothing to leave behind if the broker dies
//! mid-request.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// A `deploy/*.env` file, read for the few values the broker needs. The same
/// file `deploy.sh` reads, so there is one description of where prod is.
pub struct DeployEnv {
    pub server: String,
    pub ssh_port: u16,
    pub remote_dir: String,
    pub service: String,
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

impl DeployEnv {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path).with_context(|| {
            format!(
                "reading {} — the prod broker describes prod from the same file \
                 deploy.sh does",
                path.display()
            )
        })?;

        let get = |wanted: &str| -> Option<String> {
            raw.lines()
                .filter_map(|line| line.trim().strip_prefix(wanted)?.strip_prefix('='))
                .map(|value| unquote(value).to_string())
                .rfind(|value| !value.is_empty())
        };

        let server = get("DEPLOY_SERVER")
            .with_context(|| format!("{} has no DEPLOY_SERVER (user@host)", path.display()))?;
        Ok(Self {
            ssh_port: get("DEPLOY_SSH_PORT")
                .as_deref()
                .unwrap_or("22")
                .parse()
                .context("DEPLOY_SSH_PORT is not a port number")?,
            remote_dir: get("DEPLOY_REMOTE_DIR").unwrap_or_else(|| "/opt/myapps".into()),
            service: get("DEPLOY_SERVICE_NAME").unwrap_or_else(|| "myapps".into()),
            server,
        })
    }
}

/// Wrap a value for the remote shell. ssh joins its arguments into one string
/// and the login shell splits it again, so a path only survives intact quoted.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub struct Remote {
    env: DeployEnv,
    key: Option<PathBuf>,
    timeout: Duration,
    /// The service user that owns the database, and the only account the
    /// broker ever runs anything as.
    service_user: String,
}

/// What a remote command produced. `status` is kept so a caller can tell
/// "prod said no" from "the broker could not ask".
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub ok: bool,
}

impl Remote {
    pub fn new(
        env: DeployEnv,
        key: Option<PathBuf>,
        service_user: String,
        timeout: Duration,
    ) -> Self {
        Self {
            env,
            key,
            timeout,
            service_user,
        }
    }

    pub fn service(&self) -> &str {
        &self.env.service
    }

    /// `DEPLOY_REMOTE_DIR/data/myapps.db` — where `deploy.sh` puts it.
    pub fn database_path(&self) -> String {
        format!(
            "{}/data/myapps.db",
            self.env.remote_dir.trim_end_matches('/')
        )
    }

    fn ssh(&self, compress: bool) -> Command {
        let mut command = Command::new("ssh");
        // BatchMode above all: a broker that stops at a password or a
        // host-key prompt does not fail, it hangs, and the guest waits on a
        // request that will never answer.
        command
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=10")
            .arg("-T")
            .arg("-p")
            .arg(self.env.ssh_port.to_string());
        if compress {
            command.arg("-C");
        }
        if let Some(key) = &self.key {
            command
                .arg("-i")
                .arg(key)
                .arg("-o")
                .arg("IdentitiesOnly=yes");
        }
        command.arg(&self.env.server);
        command.stdin(Stdio::null());
        command
    }

    /// Run one remote command and collect its output. For everything that is
    /// small enough to hold — logs, `systemctl status`.
    async fn run(&self, remote: String) -> Result<Output> {
        let mut command = self.ssh(false);
        command.arg(remote);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = command.spawn().context("spawning ssh")?;
        let finished = tokio::time::timeout(self.timeout, child.wait_with_output()).await;
        let output = match finished {
            Ok(result) => result.context("waiting for ssh")?,
            // wait_with_output owns the child, so there is no handle left to
            // kill; the orphan dies on its own ConnectTimeout or when the
            // pipes close. Bounding the wait is what keeps the broker
            // answering.
            Err(_) => bail!("prod did not answer within {}s", self.timeout.as_secs()),
        };
        Ok(Output {
            ok: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }

    /// Stream `sqlite3 .dump` into a local file.
    ///
    /// Streamed rather than collected because the result is the whole
    /// database as SQL text, and buffering it would hold it in the broker's
    /// memory at full size for no reason. `max_bytes` bounds what a runaway
    /// prod can write into the host's cache directory.
    pub async fn dump_database(&self, into: &Path, max_bytes: u64) -> Result<u64> {
        let db = self.database_path();
        // -readonly is the enforcement, not the comment: sqlite refuses the
        // write rather than relying on `.dump` not attempting one. It needs
        // the -shm file to exist, which it does whenever the service is
        // running; a stopped service is the one case this reports instead.
        let remote = format!(
            "sudo -u {} sqlite3 -readonly {} .dump",
            quote(&self.service_user),
            quote(&db)
        );

        let mut command = self.ssh(true);
        command.arg(remote);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().context("spawning ssh")?;

        let mut stdout = child.stdout.take().context("no stdout on ssh")?;
        let mut stderr_pipe = child.stderr.take().context("no stderr on ssh")?;
        // Drained concurrently: a remote command that writes more to stderr
        // than the pipe buffer holds would otherwise block forever on a write
        // nobody is reading.
        let stderr = tokio::spawn(async move {
            let mut buffer = String::new();
            let _ = stderr_pipe.read_to_string(&mut buffer).await;
            buffer
        });

        let copy = async {
            let mut file = tokio::fs::File::create(into)
                .await
                .with_context(|| format!("creating {}", into.display()))?;
            let mut buffer = vec![0u8; 256 * 1024];
            let mut written = 0u64;
            loop {
                let read = stdout.read(&mut buffer).await.context("reading the dump")?;
                if read == 0 {
                    break;
                }
                written += read as u64;
                if written > max_bytes {
                    bail!(
                        "the dump passed {} MiB (--max-dump-mb) and was abandoned",
                        max_bytes / (1024 * 1024)
                    );
                }
                file.write_all(&buffer[..read])
                    .await
                    .context("writing the dump")?;
            }
            file.flush().await.context("flushing the dump")?;
            Ok(written)
        };

        let result = tokio::time::timeout(self.timeout, copy).await;
        let written = match result {
            Ok(written) => written,
            Err(_) => {
                let _ = child.kill().await;
                bail!("the dump did not finish within {}s", self.timeout.as_secs());
            }
        };

        let status = child.wait().await.context("waiting for ssh")?;
        let stderr = stderr.await.unwrap_or_default();
        let written = written?;

        if !status.success() {
            bail!(
                "prod refused the dump: {}",
                if stderr.trim().is_empty() {
                    "ssh exited non-zero with no message".to_string()
                } else {
                    stderr.trim().to_string()
                }
            );
        }
        if written == 0 {
            bail!("prod produced an empty dump: {}", stderr.trim());
        }
        Ok(written)
    }

    /// `journalctl` for one unit. Every parameter is validated by the caller
    /// against a closed set before it gets here.
    ///
    /// `--no-hostname` is not cosmetic: short-iso puts the machine's hostname
    /// on every line, and the guest is not supposed to learn where prod is.
    pub async fn logs(&self, unit: &str, since: &str, lines: u32) -> Result<Output> {
        self.run(format!(
            "sudo journalctl -u {} --since {} -n {} --no-pager --no-hostname -o short-iso",
            quote(unit),
            quote(since),
            lines
        ))
        .await
    }

    /// `systemctl status` for one unit, without the journal tail it normally
    /// appends.
    ///
    /// `-n 0` is the same promise `--no-hostname` keeps for `logs`: those
    /// trailing log lines come from the journal in its *default* format, which
    /// prints the machine's hostname on every one and which `--no-hostname`
    /// does not reach from here. Logs have their own verb.
    pub async fn status(&self, unit: &str) -> Result<Output> {
        self.run(format!(
            "sudo systemctl --no-pager --full -n 0 status {}",
            quote(unit)
        ))
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quoted_value_cannot_escape_its_literal() {
        assert_eq!(quote("myapps"), "'myapps'");
        assert_eq!(quote("a'; rm -rf /"), r"'a'\''; rm -rf /'");
    }

    #[test]
    fn unquote_strips_one_matched_pair() {
        assert_eq!(unquote("\"~/myapps-build\""), "~/myapps-build");
        assert_eq!(unquote("'/opt/myapps'"), "/opt/myapps");
        assert_eq!(unquote("  /opt/myapps  "), "/opt/myapps");
        assert_eq!(unquote("\"unbalanced"), "\"unbalanced");
    }

    #[test]
    fn the_database_path_follows_the_remote_dir() {
        let remote = Remote::new(
            DeployEnv {
                server: "deploy@odroid".into(),
                ssh_port: 22,
                remote_dir: "/opt/myapps-stage/".into(),
                service: "myapps-stage".into(),
            },
            None,
            "myapps".into(),
            Duration::from_secs(1),
        );
        assert_eq!(remote.database_path(), "/opt/myapps-stage/data/myapps.db");
    }
}
