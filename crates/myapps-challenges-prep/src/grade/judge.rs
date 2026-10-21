//! Stage `judge`: "which is harder?" for random pairs within a subject (the
//! selector's levels are per subject, so that is the ranking that matters),
//! each pair asked both ways round so `rank` can take the position bias out.
//! In the spike's simulation random pairs did as well as pairing neighbours.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::flags::progress;
use super::jsonl;
use super::llm::Llm;
use super::rubric;
use crate::grades::text_hash;
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

/// Fixed, so the same problems always get the same pairs and a resumed run
/// asks the ones still missing.
const SEED: u64 = 7;

/// One question: was `first`, shown as problem A, the harder?
#[derive(Serialize, Deserialize)]
pub struct Verdict {
    pub first: String,
    pub second: String,
    pub first_hash: String,
    pub second_hash: String,
    pub p_first: f64,
}

fn path(dir: &Path) -> PathBuf {
    dir.join("judge.jsonl")
}

pub fn load(dir: &Path) -> Result<Vec<Verdict>> {
    jsonl::read(&path(dir))
}

/// `rounds` random matchings within each subject: every round pairs each
/// problem with one other (an odd one out sits that round out). A pair drawn
/// twice is asked once. Indices are into `problems`.
pub fn pairs(problems: &[&Problem], rounds: usize) -> Vec<(usize, usize)> {
    let mut by_subject: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, p) in problems.iter().enumerate() {
        by_subject.entry(&p.subject).or_default().push(i);
    }
    let mut rng = StdRng::seed_from_u64(SEED);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for _ in 0..rounds {
        for group in by_subject.values_mut() {
            group.shuffle(&mut rng);
            for &[a, b] in group.as_chunks::<2>().0 {
                if seen.insert((a.min(b), a.max(b))) {
                    out.push((a, b));
                }
            }
        }
    }
    out
}

pub async fn run(
    llm: &Llm,
    dataset: Dataset,
    problems: &[&Problem],
    rounds: usize,
    dir: &Path,
) -> Result<()> {
    let hashes: Vec<String> = problems.iter().map(|p| text_hash(&p.problem)).collect();
    let done: HashSet<(String, String)> = load(dir)?
        .into_iter()
        .map(|v| (v.first_hash, v.second_hash))
        .collect();
    let todo: Vec<(usize, usize)> = pairs(problems, rounds)
        .into_iter()
        .flat_map(|(a, b)| [(a, b), (b, a)])
        .filter(|&(a, b)| !done.contains(&(hashes[a].clone(), hashes[b].clone())))
        .collect();
    tracing::info!(
        "judge: {} questions to go over {} problems, {rounds} rounds ({})",
        todo.len(),
        problems.len(),
        path(dir).display()
    );

    let system = format!(
        "You grade {} problems by difficulty. Answer with a single letter.",
        rubric::field(dataset)
    );
    let mut out = jsonl::Appender::open(&path(dir))?;
    let (start, mut failed) = (Instant::now(), 0);
    for (n, &(a, b)) in todo.iter().enumerate() {
        let user = format!(
            "Problem A:\n{}\n\nProblem B:\n{}\n\nWhich problem is harder to solve? Answer A or B.",
            problems[a].problem, problems[b].problem
        );
        match llm.choose(&system, &user, 2).await {
            Ok((p, _)) => out.push(&Verdict {
                first: problems[a].source_key.clone(),
                second: problems[b].source_key.clone(),
                first_hash: hashes[a].clone(),
                second_hash: hashes[b].clone(),
                p_first: p[0],
            })?,
            Err(e) => {
                tracing::warn!(
                    "judge: {} vs {} skipped: {e:#}",
                    problems[a].source_key,
                    problems[b].source_key
                );
                failed += 1;
            }
        }
        progress("judge", n + 1, todo.len(), start, 50);
    }
    tracing::info!("judge: done, {failed} skipped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(key: &str, subject: &str) -> Problem {
        Problem {
            source_key: key.into(),
            subject: subject.into(),
            topic: None,
            difficulty: 1,
            source_level: "1".into(),
            problem: key.into(),
            solution: "s".into(),
            answer: "a".into(),
            answer_type: None,
            unit: None,
            source_url: None,
            source_title: None,
        }
    }

    #[test]
    fn pairs_stay_within_a_subject_and_are_stable() {
        let owned: Vec<Problem> = (0..20)
            .map(|i| problem(&format!("p{i}"), if i < 11 { "A" } else { "B" }))
            .collect();
        let problems: Vec<&Problem> = owned.iter().collect();
        let pairs = pairs(&problems, 4);
        assert!(
            pairs
                .iter()
                .all(|&(a, b)| problems[a].subject == problems[b].subject && a != b)
        );
        let unordered: HashSet<_> = pairs.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        assert_eq!(unordered.len(), pairs.len());
        // Five pairs from 11 problems and four from 9, per round, minus repeats.
        assert!(pairs.len() <= 4 * 9 && pairs.len() > 4 * 9 / 2);
        assert_eq!(super::pairs(&problems, 4), pairs);
    }
}
