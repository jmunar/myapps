//! Prepare a Challenges dataset on a workstation, into a bundle the server
//! loads with `myapps import --app challenges --dataset <file>`.
//!
//! Everything too heavy for the server happens here; the bundle format, and
//! the checks the server applies to it, are in `myapps_challenges::bundle`.
//!
//! `bundle` runs its stages in order: fetch and map every row, render the
//! Asymptote diagrams they contain, drop the problems whose own diagram
//! failed, attach the committed grades, write. `grade` produces those grades
//! (see `grade`).

mod fetch;
mod grade;
mod grades;
mod render;
mod source;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

use myapps_challenges::bundle::{self, Diagram, Problem, Tag};
use myapps_challenges::dataset::Dataset;
use myapps_challenges::diagram::{self, Segment};
use source::Skip;

#[derive(Parser)]
#[command(about = "Prepare Challenges dataset bundles for `myapps import`, and grade them")]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Where fetched pages, rendered diagrams and grader answers are kept
    /// between runs [default: ~/.cache/myapps-challenges-prep]
    #[arg(long, global = true)]
    cache: Option<PathBuf>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Prepare a dataset's bundle
    Bundle(BundleArgs),
    /// Grade a dataset's problems, one stage at a time
    Grade(grade::Args),
}

#[derive(clap::Args)]
struct BundleArgs {
    /// Dataset key (ugphysics, hendrycks-math)
    dataset: String,
    /// Where to write the bundle [default: <dataset>.sqlite]
    #[arg(long, short)]
    out: Option<PathBuf>,
    /// Fetch every page again instead of reusing the cached ones
    #[arg(long)]
    refetch: bool,
    /// Extra Asymptote modules (olympiad.asy, cse5.asy, TrigMacros.asy from
    /// the AoPS wiki) [default: <cache>/modules]
    #[arg(long)]
    modules: Option<PathBuf>,
    /// Diagrams rendered at once [default: the number of CPUs]
    #[arg(long, short)]
    jobs: Option<usize>,
    /// Render again the diagrams that failed on an earlier run
    #[arg(long)]
    retry_failed: bool,
    /// The grader's output for this dataset
    /// [default: grades/<dataset>-v1.jsonl in this crate, if it exists]
    #[arg(long)]
    grades: Option<PathBuf>,
}

#[derive(Default)]
struct Counts {
    no_level: usize,
    diagram: usize,
    malformed: usize,
    truncated: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cache = match &cli.cache {
        Some(dir) => dir.clone(),
        None => std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .context("no cache directory: set --cache")?
            .join("myapps-challenges-prep"),
    };
    match &cli.command {
        Command::Bundle(args) => bundle(args, &cache).await,
        Command::Grade(args) => {
            let dataset = parse_dataset(&args.dataset)?;
            // The same fetch and mapping as `bundle`, so the grader sees, and
            // hashes, exactly the text the bundle will carry.
            let rows = fetch::Cache::new(cache.join("rows"), false);
            let problems = fetch_all(dataset, &rows, &mut Counts::default()).await?;
            grade::run(args, dataset, &problems, &cache).await
        }
    }
}

fn parse_dataset(key: &str) -> Result<Dataset> {
    let Some(dataset) = Dataset::from_key(key) else {
        let valid: Vec<&str> = Dataset::ALL.iter().map(|d| d.key()).collect();
        bail!(
            "Unknown dataset '{key}'; expected one of: {}",
            valid.join(", ")
        );
    };
    Ok(dataset)
}

async fn bundle(args: &BundleArgs, cache: &Path) -> Result<()> {
    let dataset = parse_dataset(&args.dataset)?;
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{}.sqlite", dataset.key())));

    let mut counts = Counts::default();
    let rows = fetch::Cache::new(cache.join("rows"), args.refetch);
    let mut problems = fetch_all(dataset, &rows, &mut counts).await?;
    let diagrams = render_diagrams(args, cache, &mut problems, &mut counts).await?;
    let tags = attach_grades(args, dataset, &problems)?;

    let writer = bundle::Writer::create(
        &out,
        dataset,
        &dataset.url(),
        dataset.license(),
        concat!("myapps-challenges-prep ", env!("CARGO_PKG_VERSION")),
    )
    .await?;
    for chunk in problems.chunks(500) {
        writer.insert(chunk).await?;
    }
    writer.insert_diagrams(&diagrams).await?;
    writer.insert_tags(&tags).await?;
    writer.finish().await?;

    tracing::info!(
        "{}: {} problems, {} diagrams and {} tags written to {}; skipped {} without a \
         level, {} whose diagram failed to render, {} malformed, {} truncated",
        dataset.name(),
        problems.len(),
        diagrams.len(),
        tags.len(),
        out.display(),
        counts.no_level,
        counts.diagram,
        counts.malformed,
        counts.truncated,
    );
    Ok(())
}

async fn fetch_all(
    dataset: Dataset,
    rows: &fetch::Cache,
    counts: &mut Counts,
) -> Result<Vec<Problem>> {
    let client = fetch::client()?;
    let mut problems = Vec::new();
    for (config, split) in source::sources(dataset) {
        let mut offset = 0;
        loop {
            let page = rows.page(&client, dataset, config, split, offset).await?;
            for entry in page.rows {
                if !entry.truncated_cells.is_empty() {
                    tracing::warn!(
                        "{config}/{split} row {}: truncated cells {:?}, skipped",
                        entry.row_idx,
                        entry.truncated_cells
                    );
                    counts.truncated += 1;
                    continue;
                }
                match source::map_row(dataset, config, split, entry.row_idx, &entry.row) {
                    Ok(p) => problems.push(p),
                    Err(Skip::NoLevel) => counts.no_level += 1,
                    Err(Skip::Malformed) => {
                        tracing::warn!(
                            "{config}/{split} row {}: malformed, skipped",
                            entry.row_idx
                        );
                        counts.malformed += 1;
                    }
                }
            }
            offset += fetch::PAGE;
            if offset >= page.num_rows_total {
                break;
            }
        }
        tracing::info!("{}: {config}/{split} fetched", dataset.name());
    }
    Ok(problems)
}

/// The tags from the grades file, for the problems it still matches. A missing
/// default file just means the dataset has not been graded; a missing file
/// passed with `--grades` is an error.
fn attach_grades(args: &BundleArgs, dataset: Dataset, problems: &[Problem]) -> Result<Vec<Tag>> {
    let path = match &args.grades {
        Some(path) => path.clone(),
        None => {
            let path = grades::default_path(dataset);
            if !path.exists() {
                tracing::info!(
                    "{}: not graded ({} not found)",
                    dataset.name(),
                    path.display()
                );
                return Ok(Vec::new());
            }
            path
        }
    };
    let rows = grades::read(&path)?;
    let m = grades::match_problems(problems, &rows);
    tracing::info!(
        "{}: {} of {} problems graded from {}",
        dataset.name(),
        m.graded,
        problems.len(),
        path.display()
    );
    if m.stale > 0 || m.unknown > 0 {
        tracing::warn!(
            "{}: {} grades skipped because the problem's text changed since, {} for problems \
             this preparation does not have; grade them again",
            dataset.name(),
            m.stale,
            m.unknown
        );
    }
    Ok(m.tags)
}

/// Render every diagram in `problems`, drop the problems that cannot be shown
/// without one that failed, and return the diagrams the rest use.
async fn render_diagrams(
    args: &BundleArgs,
    cache: &Path,
    problems: &mut Vec<Problem>,
    counts: &mut Counts,
) -> Result<Vec<Diagram>> {
    let mut blocks = BTreeMap::new();
    for p in problems.iter() {
        for text in [&p.problem, &p.solution] {
            for segment in diagram::split(text) {
                if let Segment::Diagram { source, hash } = segment {
                    blocks.entry(hash).or_insert_with(|| source.to_string());
                }
            }
        }
    }
    if blocks.is_empty() {
        return Ok(Vec::new());
    }

    let modules = args
        .modules
        .clone()
        .unwrap_or_else(|| cache.join("modules"));
    let jobs = args
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()));
    let renderer =
        render::Renderer::new(&cache.join("asy"), &modules, jobs, args.retry_failed).await?;
    let rendered = renderer.render_all(blocks.into_iter().collect()).await?;

    let before = problems.len();
    problems.retain(|p| {
        diagram::hashes(&p.problem)
            .iter()
            .all(|h| rendered.contains_key(h))
    });
    counts.diagram = before - problems.len();

    let used: HashSet<String> = problems
        .iter()
        .flat_map(|p| {
            let mut hashes = diagram::hashes(&p.problem);
            hashes.extend(diagram::hashes(&p.solution));
            hashes
        })
        .collect();
    let mut diagrams: Vec<Diagram> = rendered
        .into_iter()
        .filter(|(hash, _)| used.contains(hash))
        .map(|(hash, svg)| Diagram { hash, svg })
        .collect();
    diagrams.sort_by(|a, b| a.hash.cmp(&b.hash));
    Ok(diagrams)
}
