//! Render Asymptote blocks to SVG with the workstation's `asy`.
//!
//! Each block runs in its own bubblewrap sandbox: no network, a read-only
//! system, an empty `/home` and nothing writable but its work directory. The
//! blocks come from a public dataset, and while `asy` disables its own system
//! calls by default, it can still read files, and a label could carry one
//! into the SVG that ends up on the server.
//!
//! The blocks were written for AoPS, which provides its own modules
//! (`olympiad`, `cse5`, `TrigMacros`) and imports the first two implicitly.
//! Those are not part of Asymptote and are not ours to redistribute, so they
//! are read from a local modules directory (`--modules`), and imported
//! implicitly here too when present.
//!
//! Results are cached by hash under a directory named for the preamble, the
//! modules and the `asy` version, so a rerun renders only what is new, and
//! changing any of them renders everything again. Failures are cached too, with the error, as
//! `<hash>.err`; `--retry-failed` tries them again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use myapps_challenges::diagram;

/// Ahead of every block. 3D scenes (`import three`) are drawn as vectors
/// rather than through OpenGL, which SVG output cannot use.
const SETTINGS: &str = "settings.render = 0;\nsettings.prc = false;\n";

/// For a block that sets no size of its own. Asymptote's default is one
/// point per unit, which draws a triangle with sides of 10 at the height of a
/// label; this fits the drawing in 180 x 180 pt, about 240 px.
const DEFAULT_SIZE: &str = "size(180);\n";

/// The AoPS modules its blocks use without importing.
const IMPLICIT: [&str; 2] = ["olympiad", "cse5"];

const TIMEOUT: Duration = Duration::from_secs(60);

pub struct Renderer {
    setup: Arc<Setup>,
    jobs: usize,
}

struct Setup {
    dir: PathBuf,
    preamble: String,
    modules: Option<PathBuf>,
    retry_failed: bool,
}

impl Renderer {
    pub async fn new(
        cache_root: &Path,
        modules: &Path,
        jobs: usize,
        retry_failed: bool,
    ) -> Result<Renderer> {
        let version = tool_version("asy").await?;
        tool_version("bwrap").await?;

        // Every module's contents go into the cache key, so adding or
        // changing one renders again.
        let mut key = format!("{SETTINGS}\n{DEFAULT_SIZE}\n{version}\n");
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(modules) {
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "asy"))
                .collect();
            files.sort();
            for file in files {
                let name = file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let contents = std::fs::read_to_string(&file)
                    .with_context(|| format!("reading {}", file.display()))?;
                key.push_str(&format!("{name} {}\n", diagram::hash(&contents)));
                found.push(name);
            }
        }
        let mut preamble = SETTINGS.to_string();
        for module in IMPLICIT {
            if found.iter().any(|f| f == module) {
                preamble.push_str(&format!("import {module};\n"));
            } else {
                tracing::warn!(
                    "{module}.asy is not in {}: blocks that rely on it will fail",
                    modules.display()
                );
            }
        }
        key.push_str(&preamble);

        let dir = cache_root.join(&diagram::hash(&key)[..16]);
        std::fs::create_dir_all(dir.join("work"))
            .with_context(|| format!("creating {}", dir.display()))?;
        tracing::info!(
            "Rendering with {version}, modules [{}]; cache in {}",
            found.join(", "),
            dir.display()
        );
        Ok(Renderer {
            setup: Arc::new(Setup {
                dir,
                preamble,
                modules: (!found.is_empty()).then(|| modules.to_path_buf()),
                retry_failed,
            }),
            jobs: jobs.max(1),
        })
    }

    /// Render every `(hash, source)`, returning the SVGs that succeeded by
    /// hash; the failures are logged.
    pub async fn render_all(
        &self,
        blocks: Vec<(String, String)>,
    ) -> Result<HashMap<String, String>> {
        let total = blocks.len();
        let permits = Arc::new(Semaphore::new(self.jobs));
        let mut tasks = JoinSet::new();
        for (hash, source) in blocks {
            let permits = permits.clone();
            let setup = self.setup.clone();
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await;
                let result = render_cached(&setup, &hash, &source).await;
                (hash, result)
            });
        }

        let mut svgs = HashMap::new();
        let mut failed = 0;
        while let Some(joined) = tasks.join_next().await {
            let (hash, result) = joined?;
            match result {
                Ok(svg) => {
                    svgs.insert(hash, svg);
                }
                Err(e) => {
                    tracing::debug!("{hash}: {e:#}");
                    failed += 1;
                }
            }
            let done = svgs.len() + failed;
            if done % 100 == 0 || done == total {
                tracing::info!("Diagrams: {done}/{total} ({failed} failed)");
            }
        }
        if failed > 0 {
            tracing::warn!(
                "{failed} diagrams failed to render; the errors are in {}/*.err",
                self.setup.dir.display()
            );
        }
        Ok(svgs)
    }
}

async fn tool_version(tool: &str) -> Result<String> {
    let out = Command::new(tool)
        .arg("--version")
        .output()
        .await
        .with_context(|| format!("running {tool}: is it installed?"))?;
    let text = String::from_utf8_lossy(if out.stdout.is_empty() {
        &out.stderr
    } else {
        &out.stdout
    })
    .lines()
    .next()
    .unwrap_or_default()
    .trim()
    .to_string();
    if text.is_empty() {
        bail!("{tool} --version printed nothing");
    }
    Ok(text)
}

async fn render_cached(setup: &Setup, hash: &str, source: &str) -> Result<String> {
    let dir = &setup.dir;
    let svg_path = dir.join(format!("{hash}.svg"));
    let err_path = dir.join(format!("{hash}.err"));
    if let Ok(svg) = tokio::fs::read_to_string(&svg_path).await {
        return Ok(svg);
    }
    if !setup.retry_failed
        && let Ok(err) = tokio::fs::read_to_string(&err_path).await
    {
        bail!("failed before: {}", err.lines().next().unwrap_or_default());
    }

    let work = dir.join("work").join(hash);
    let result = render(setup, &work, source).await;
    let _ = tokio::fs::remove_dir_all(&work).await;
    match result {
        Ok(svg) => {
            tokio::fs::write(&svg_path, &svg).await?;
            let _ = tokio::fs::remove_file(&err_path).await;
            Ok(svg)
        }
        Err(e) => {
            tokio::fs::write(&err_path, format!("{e:#}\n\n--- source ---\n{source}\n")).await?;
            Err(e)
        }
    }
}

async fn render(setup: &Setup, work: &Path, source: &str) -> Result<String> {
    tokio::fs::create_dir_all(work).await?;
    let size = if sets_size(source) { "" } else { DEFAULT_SIZE };
    tokio::fs::write(
        work.join("d.asy"),
        format!("{}{size}{source}\n", setup.preamble),
    )
    .await?;

    let work_str = work.to_str().context("non-UTF-8 cache path")?;
    let mut cmd = Command::new("bwrap");
    cmd.args(["--ro-bind", "/", "/"])
        .args(["--dev", "/dev", "--proc", "/proc"])
        .args(["--tmpfs", "/tmp", "--tmpfs", "/run"])
        .args(["--tmpfs", "/home", "--tmpfs", "/root"])
        .args(["--bind", work_str, work_str])
        .args(["--chdir", work_str])
        .args(["--setenv", "HOME", work_str]);
    if let Some(modules) = &setup.modules {
        let modules = modules.to_str().context("non-UTF-8 modules path")?;
        cmd.args(["--ro-bind", modules, modules])
            .args(["--setenv", "ASYMPTOTE_DIR", modules]);
    }
    cmd.args(["--unshare-all", "--die-with-parent", "--new-session"])
        .args(["asy", "-noV", "-safe", "-f", "svg", "-o", "d", "d.asy"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let out = tokio::time::timeout(TIMEOUT, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("timed out after {}s", TIMEOUT.as_secs()))??;
    if !out.status.success() {
        // Lead with asy's own error line (`d.asy: 12.3: …` or `error: …`),
        // which is what a failure gets grouped by; the rest is context.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
        let first = lines
            .iter()
            .find(|l| l.starts_with("d.asy:") || l.starts_with("error"))
            .or(lines.last())
            .copied()
            .unwrap_or("no output");
        bail!(
            "asy exited with {}: {first}\n\n{}",
            out.status,
            lines.join("\n")
        );
    }
    let svg = tokio::fs::read_to_string(work.join("d.svg"))
        .await
        .context("asy succeeded but wrote no d.svg")?;
    if !diagram::is_safe_svg(&svg) {
        bail!("the SVG contains active content");
    }
    Ok(svg)
}

/// Whether `source` sizes its own picture (`size`, `unitsize`, `size3`, …).
fn sets_size(source: &str) -> bool {
    source.match_indices("size").any(|(i, _)| {
        let after = source[i + 4..].trim_start_matches(|c: char| c.is_ascii_digit());
        after.trim_start().starts_with('(')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_a_block_sizing_itself() {
        assert!(sets_size("size(200); draw(A--B);"));
        assert!(sets_size("unitsize(0.5cm);"));
        assert!(sets_size("size3 (100);"));
        assert!(!sets_size("draw(A--B); label(\"size\", A);"));
        assert!(!sets_size("real fontsize = 10;"));
    }
}
