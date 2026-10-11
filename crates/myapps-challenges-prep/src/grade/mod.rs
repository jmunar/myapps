//! The problem grader: a difficulty estimate per problem, written to the
//! committed grades file that `bundle` turns into tags.
//!
//! Three stages, run one at a time against one `llama-server` chat model, each
//! caching every answer as it arrives so a killed run resumes where it stopped
//! (docs/ideas-03-graded-math-physics.md, "Grader spike"):
//!
//! - `flags` asks each problem the dataset's rubric questions.
//! - `judge` asks "which is harder?" of random pairs within a subject, both
//!   ways round.
//! - `rank` makes no model calls: a ridge regression from the flags and
//!   lengths to the dataset's levels gives each problem a prior, Bradley-Terry
//!   over the verdicts refines it, and the result is reported and written.

mod flags;
mod jsonl;
mod judge;
mod llm;
mod rank;
mod rubric;
mod stats;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::grades;
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

/// The model the spike measured, and so the default.
const DEFAULT_MODEL: &str = "Qwen3-8B-Q4_K_M.gguf";

#[derive(clap::Args)]
pub struct Args {
    pub stage: Stage,
    /// Dataset key (ugphysics, hendrycks-math)
    pub dataset: String,
    /// The llama-server to ask (flags, judge)
    #[arg(long, default_value = "http://127.0.0.1:8091")]
    pub server: String,
    /// The model file llama-server must have loaded; answers are cached per
    /// model, so `rank` uses it too
    #[arg(long, default_value = DEFAULT_MODEL)]
    pub model: String,
    /// Only this many problems, the same ones every run (a stable shuffle)
    #[arg(long)]
    pub limit: Option<usize>,
    /// Only this subject
    #[arg(long)]
    pub subject: Option<String>,
    /// Judge rounds: each pairs every problem in a subject with one other, so
    /// this is the number of comparisons per problem (judge, rank)
    #[arg(long, default_value_t = 6)]
    pub rounds: usize,
    /// How far, in Bradley-Terry units, a grade may stray from its prior (rank)
    #[arg(long, default_value_t = 1.0)]
    pub tau: f64,
    /// Where `rank` writes the grades [default: grades/<dataset>-v1.jsonl in
    /// this crate, and only for a run over every problem]
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Stage {
    Flags,
    Judge,
    Rank,
}

pub async fn run(args: &Args, dataset: Dataset, all: &[Problem], cache: &Path) -> Result<()> {
    let problems = select(all, args.limit, args.subject.as_deref());
    tracing::info!(
        "{}: grading {} of {} problems",
        dataset.name(),
        problems.len(),
        all.len()
    );
    let dir = cache
        .join("grade")
        .join(dataset.key())
        .join(args.model.trim_end_matches(".gguf"));
    std::fs::create_dir_all(&dir)?;

    match args.stage {
        Stage::Flags => {
            let llm = llm::Llm::connect(&args.server, &args.model).await?;
            flags::run(&llm, dataset, &problems, &dir).await
        }
        Stage::Judge => {
            let llm = llm::Llm::connect(&args.server, &args.model).await?;
            judge::run(&llm, dataset, &problems, args.rounds, &dir).await
        }
        Stage::Rank => {
            let whole = args.limit.is_none() && args.subject.is_none();
            let out = match &args.out {
                Some(out) => Some(out.clone()),
                None if whole => Some(grades::default_path(dataset)),
                None => None,
            };
            rank::run(&rank::Options {
                dataset,
                problems: &problems,
                dir: &dir,
                model: &args.model,
                rounds: args.rounds,
                tau: args.tau,
                out: out.as_deref(),
            })
        }
    }
}

/// The problems to grade, in a stable pseudo-random order (by the hash of
/// their key), so `--limit` takes the same spread of subjects and levels on
/// every run and a resumed run carries on with the same ones.
fn select<'a>(all: &'a [Problem], limit: Option<usize>, subject: Option<&str>) -> Vec<&'a Problem> {
    let mut chosen: Vec<(String, &Problem)> = all
        .iter()
        .filter(|p| subject.is_none_or(|s| p.subject == s))
        .map(|p| (grades::text_hash(&p.source_key), p))
        .collect();
    chosen.sort_by(|a, b| a.0.cmp(&b.0));
    chosen.truncate(limit.unwrap_or(usize::MAX));
    chosen.into_iter().map(|(_, p)| p).collect()
}
