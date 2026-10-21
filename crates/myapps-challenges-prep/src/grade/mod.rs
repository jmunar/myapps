//! The problem grader: a difficulty estimate per problem, written to the
//! committed grades file that `bundle` turns into tags.
//!
//! Stages, run one at a time against one `llama-server` chat model, each
//! caching every answer as it arrives so a killed run resumes where it stopped
//! (docs/ideas-03-graded-math-physics.md, "Grader spike"):
//!
//! - `flags` asks each problem the dataset's rubric questions.
//! - `judge` asks "which is harder?" of random pairs within a subject, both
//!   ways round.
//! - `solve` has a stronger model attempt a sample of the problems, as a
//!   check on the grades that needs no expert; it is not part of them.
//! - `check` has one model judge another solver's answers, so two solvers
//!   are compared under the same checker.
//! - `rank` makes no model calls: a ridge regression from the flags and
//!   lengths to the dataset's levels gives each problem a prior, Bradley-Terry
//!   over the verdicts refines it, and the result is reported, against the
//!   solver's results too when there are some, and written.

mod flags;
mod jsonl;
mod judge;
mod llm;
mod rank;
mod rubric;
mod solve;
mod stats;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::grades;
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

/// The model the spike measured, and so the default.
const DEFAULT_MODEL: &str = "Qwen3-8B-Q4_K_M.gguf";
/// The strongest at maths and science that runs well on the dev laptop's
/// iGPU (a 3B-active MoE: ~21 tokens/s, against ~13 for a dense 9B).
const DEFAULT_SOLVER: &str = "Qwen3.6-35B-A3B-UD-Q4_K_M.gguf";

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
    /// The model file `solve` needs loaded, and whose results `rank` reports
    #[arg(long, default_value = DEFAULT_SOLVER)]
    pub solver: String,
    /// The model whose verdicts on the answers `rank` uses; `check` needs it
    /// loaded, and asks it about the answers of `--solver`
    #[arg(long, default_value = DEFAULT_SOLVER)]
    pub checker: String,
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
    /// Attempts per problem (solve, each with its own seed)
    #[arg(long, default_value_t = 1)]
    pub attempts: u64,
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
    Solve,
    Check,
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
    let model_dir = |model: &str| {
        cache
            .join("grade")
            .join(dataset.key())
            .join(model.trim_end_matches(".gguf"))
    };
    let dir = model_dir(&args.model);
    let solver_dir = model_dir(&args.solver);
    std::fs::create_dir_all(&dir)?;

    match args.stage {
        Stage::Flags => {
            let llm = llm::Llm::connect(&args.server, &args.model, "-c 8192").await?;
            flags::run(&llm, dataset, &problems, &dir).await
        }
        Stage::Judge => {
            let llm = llm::Llm::connect(&args.server, &args.model, "-c 8192").await?;
            judge::run(&llm, dataset, &problems, args.rounds, &dir).await
        }
        Stage::Solve => {
            std::fs::create_dir_all(&solver_dir)?;
            let llm =
                llm::Llm::connect(&args.server, &args.solver, &solve::server_options()).await?;
            solve::run(
                &llm,
                dataset,
                &problems,
                &args.solver,
                args.attempts,
                &solver_dir,
            )
            .await
        }
        Stage::Check => {
            let llm = llm::Llm::connect(&args.server, &args.checker, "-c 8192").await?;
            solve::check_all(&llm, dataset, &problems, &solver_dir, &args.checker).await
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
                solver_dir: &solver_dir,
                solver: &args.solver,
                checker: &args.checker,
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
