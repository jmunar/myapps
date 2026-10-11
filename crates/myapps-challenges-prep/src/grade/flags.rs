//! Stage `flags`: the rubric's answers for every problem, cached per rubric
//! version and keyed on the problem's text hash, so an edited problem is asked
//! again and a killed run carries on.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::jsonl;
use super::llm::Llm;
use super::rubric;
use crate::grades::text_hash;
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

#[derive(Serialize, Deserialize)]
pub struct Row {
    pub source_key: String,
    pub text_hash: String,
    /// Question kind → answer value.
    pub values: BTreeMap<String, f64>,
    /// The mean probability mass on the option letters, over the questions.
    pub mass: f64,
}

fn path(dir: &Path) -> PathBuf {
    dir.join(format!("flags-{}.jsonl", rubric::VERSION))
}

/// The cached answers, by (source_key, text_hash).
pub fn load(dir: &Path) -> Result<HashMap<(String, String), Row>> {
    Ok(jsonl::read::<Row>(&path(dir))?
        .into_iter()
        .map(|r| ((r.source_key.clone(), r.text_hash.clone()), r))
        .collect())
}

pub async fn run(llm: &Llm, dataset: Dataset, problems: &[&Problem], dir: &Path) -> Result<()> {
    let done = load(dir)?;
    let todo: Vec<&Problem> = problems
        .iter()
        .copied()
        .filter(|p| !done.contains_key(&(p.source_key.clone(), text_hash(&p.problem))))
        .collect();
    tracing::info!(
        "flags: {} already answered, {} to go ({})",
        problems.len() - todo.len(),
        todo.len(),
        path(dir).display()
    );

    let system = rubric::system(dataset);
    let questions = rubric::questions(dataset);
    let mut out = jsonl::Appender::open(&path(dir))?;
    let (start, mut failed) = (Instant::now(), 0);
    for (n, p) in todo.iter().enumerate() {
        let mut values = BTreeMap::new();
        let mut mass = 0.0;
        let mut error = None;
        for q in &questions {
            match llm
                .choose(&system, &rubric::prompt(&p.problem, q), q.options.len())
                .await
            {
                Ok((probs, m)) => {
                    values.insert(q.kind.to_string(), rubric::value(q, &probs));
                    mass += m;
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = error {
            // Most likely a problem longer than the context; it stays
            // unflagged, and so ungraded, rather than stopping the run.
            tracing::warn!("flags: {} skipped: {e:#}", p.source_key);
            failed += 1;
            continue;
        }
        out.push(&Row {
            source_key: p.source_key.clone(),
            text_hash: text_hash(&p.problem),
            values,
            mass: mass / questions.len() as f64,
        })?;
        progress("flags", n + 1, todo.len(), start);
    }
    tracing::info!("flags: done, {failed} skipped");
    Ok(())
}

/// A progress line every 50 items, with the time left at the rate so far.
pub fn progress(stage: &str, done: usize, total: usize, start: Instant) {
    if !done.is_multiple_of(50) && done != total {
        return;
    }
    let per = start.elapsed().as_secs_f64() / done as f64;
    let left = ((total - done) as f64 * per) as u64;
    tracing::info!(
        "{stage}: {done}/{total}, {per:.2} s each, {}h{:02}m left",
        left / 3600,
        left % 3600 / 60
    );
}
