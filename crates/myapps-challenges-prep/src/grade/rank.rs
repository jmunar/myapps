//! Stage `rank`: no model calls. The prior is a ridge regression from the
//! flags and lengths to the dataset's own levels, out-of-fold so no problem's
//! prior saw its own level; Bradley-Terry over the judge's verdicts refines it.
//! Reports how both agree with the levels, and writes the grades file.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::stats::{self, Obs};
use super::{flags, judge, rubric};
use crate::grades::{self, text_hash};
use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

const FOLDS: usize = 5;

pub struct Options<'a> {
    pub dataset: Dataset,
    pub problems: &'a [&'a Problem],
    pub dir: &'a Path,
    pub model: &'a str,
    pub rounds: usize,
    pub tau: f64,
    /// Where to write the grades; `None` only reports.
    pub out: Option<&'a Path>,
}

/// A hand-checked pair, in `grades/<dataset>-checks.jsonl`.
#[derive(Deserialize)]
struct Check {
    harder: String,
    easier: String,
}

pub fn run(o: &Options) -> Result<()> {
    let questions = rubric::questions(o.dataset);
    let answered = flags::load(o.dir)?;

    // The problems with flags for their current text; the rest can't be graded.
    let mut graded: Vec<(&Problem, String, &flags::Row)> = Vec::new();
    for p in o.problems {
        let hash = text_hash(&p.problem);
        if let Some(row) = answered.get(&(p.source_key.clone(), hash.clone())) {
            graded.push((p, hash, row));
        }
    }
    anyhow::ensure!(
        graded.len() >= 2 * FOLDS,
        "only {} of {} problems have flags: run `grade flags` first",
        graded.len(),
        o.problems.len()
    );
    let level: Vec<f64> = graded.iter().map(|(p, ..)| p.difficulty as f64).collect();
    let flag = |k: &str| -> Vec<f64> {
        graded
            .iter()
            .map(|(_, _, r)| r.values.get(k).copied().unwrap_or(f64::NAN))
            .collect()
    };

    // The prior.
    let x: Vec<Vec<f64>> = graded
        .iter()
        .map(|(p, _, r)| {
            let mut row: Vec<f64> = questions.iter().map(|q| r.values[q.kind]).collect();
            row.push((p.problem.chars().count() as f64).ln());
            row.push((p.solution.chars().count().max(1) as f64).ln());
            row
        })
        .collect();
    let fold: Vec<usize> = graded
        .iter()
        .map(|(p, ..)| usize::from_str_radix(&text_hash(&p.source_key)[..8], 16).unwrap() % FOLDS)
        .collect();
    let (prior, lambda) = stats::ridge_out_of_fold(&x, &level, &fold);

    // The verdicts on graded problems, both still the text that was judged.
    let index: HashMap<(&str, &str), usize> = graded
        .iter()
        .enumerate()
        .map(|(i, (p, hash, _))| ((p.source_key.as_str(), hash.as_str()), i))
        .collect();
    let verdicts = judge::load(o.dir)?;
    let obs: Vec<Obs> = verdicts
        .iter()
        .filter_map(|v| {
            Some(Obs {
                first: *index.get(&(v.first.as_str(), v.first_hash.as_str()))?,
                second: *index.get(&(v.second.as_str(), v.second_hash.as_str()))?,
                p: v.p_first,
            })
        })
        .collect();
    let fit = stats::bradley_terry(&prior, &obs, o.tau);
    // With no verdicts the fit has no scale to find; the grade is the prior.
    let grade = if obs.is_empty() {
        prior.clone()
    } else {
        fit.score.clone()
    };

    report(
        o, &graded, &level, &prior, &grade, &obs, &flag, lambda, &fit,
    );

    if let Some(out) = o.out {
        let run = format!(
            "flags {}, {} verdicts ({} rounds), ridge λ={lambda}, tau={}",
            rubric::VERSION,
            obs.len(),
            o.rounds,
            o.tau
        );
        let mut text = String::new();
        let mut rows: Vec<usize> = (0..graded.len()).collect();
        rows.sort_by(|&a, &b| graded[a].0.source_key.cmp(&graded[b].0.source_key));
        for i in rows {
            let (p, hash, r) = &graded[i];
            let mut tags = BTreeMap::from([
                ("grade".to_string(), round(grade[i])),
                ("grade_prior".to_string(), round(prior[i])),
            ]);
            for (k, v) in &r.values {
                tags.insert(format!("rubric.{k}"), round(*v));
            }
            let row = grades::Row {
                source_key: p.source_key.clone(),
                text_hash: hash.clone(),
                tags,
                model: o.model.to_string(),
                rubric: rubric::VERSION.to_string(),
                run: run.clone(),
            };
            text.push_str(&serde_json::to_string(&row)?);
            text.push('\n');
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let partial = out.with_extension("jsonl.partial");
        std::fs::write(&partial, text)?;
        std::fs::rename(&partial, out).with_context(|| format!("writing {}", out.display()))?;
        tracing::info!("rank: {} grades written to {}", graded.len(), out.display());
    } else {
        tracing::info!(
            "rank: a partial run (--limit or --subject) only reports; pass --out to write"
        );
    }
    Ok(())
}

/// Four decimals are plenty, and keep the committed file's diffs readable.
fn round(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

#[allow(clippy::too_many_arguments)]
fn report(
    o: &Options,
    graded: &[(&Problem, String, &flags::Row)],
    level: &[f64],
    prior: &[f64],
    grade: &[f64],
    obs: &[Obs],
    flag: &dyn Fn(&str) -> Vec<f64>,
    lambda: f64,
    fit: &stats::Fit,
) {
    let mass = graded.iter().map(|(_, _, r)| r.mass).sum::<f64>() / graded.len() as f64;
    println!(
        "\n{}: {} of {} problems have flags (mass on the letters {mass:.3}); {} verdicts",
        o.dataset.name(),
        graded.len(),
        o.problems.len(),
        obs.len()
    );
    println!("\nSpearman of each flag with the level:");
    let line: Vec<String> = rubric::questions(o.dataset)
        .iter()
        .map(|q| format!("{} {:+.2}", q.kind, stats::spearman(&flag(q.kind), level)))
        .collect();
    println!("  {}", line.join("  "));

    println!("\nSpearman with the level (prior: ridge λ={lambda}, out-of-fold):");
    println!(
        "  {:32} {:>6} {:>7} {:>7}",
        "subject", "n", "prior", "grade"
    );
    let mut subjects: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (p, ..)) in graded.iter().enumerate() {
        subjects.entry(&p.subject).or_default().push(i);
    }
    let pick = |v: &[f64], idx: &[usize]| -> Vec<f64> { idx.iter().map(|&i| v[i]).collect() };
    for (subject, idx) in &subjects {
        let l = pick(level, idx);
        println!(
            "  {subject:32} {:>6} {:>+7.2} {:>+7.2}",
            idx.len(),
            stats::spearman(&pick(prior, idx), &l),
            stats::spearman(&pick(grade, idx), &l)
        );
    }
    println!(
        "  {:32} {:>6} {:>+7.2} {:>+7.2}",
        "all",
        graded.len(),
        stats::spearman(prior, level),
        stats::spearman(grade, level)
    );

    if !obs.is_empty() {
        // Each pair asked both ways round: does the judge name the same winner,
        // and does the order-averaged verdict agree with the levels?
        let by_pair: HashMap<(usize, usize), f64> =
            obs.iter().map(|v| ((v.first, v.second), v.p)).collect();
        let (mut both, mut same, mut decided, mut right) = (0, 0, 0, 0);
        for (&(a, b), &p_ab) in &by_pair {
            let Some(&p_ba) = by_pair.get(&(b, a)) else {
                continue;
            };
            if a > b {
                continue;
            }
            both += 1;
            if (p_ab > 0.5) == (p_ba < 0.5) {
                same += 1;
            }
            if level[a] != level[b] {
                decided += 1;
                if ((p_ab + 1.0 - p_ba) / 2.0 > 0.5) == (level[a] > level[b]) {
                    right += 1;
                }
            }
        }
        let first = obs.iter().map(|v| v.p).sum::<f64>() / obs.len() as f64;
        println!(
            "\nJudge: {both} pairs asked both ways; same winner both ways {:.0}%; \
             says \"A\" {:.0}% on average; order-averaged agreement with the level {:.0}% \
             (of {decided} pairs of different levels)",
            100.0 * same as f64 / both.max(1) as f64,
            100.0 * first,
            100.0 * right as f64 / decided.max(1) as f64,
        );
        println!(
            "Bradley-Terry (tau {}): position bias {:+.2}, {:.2} logits per prior unit",
            o.tau, fit.position, fit.scale
        );
    }

    let checks_path =
        grades::default_path(o.dataset).with_file_name(format!("{}-checks.jsonl", o.dataset.key()));
    if let Ok(checks) = super::jsonl::read::<Check>(&checks_path)
        && !checks.is_empty()
    {
        let at: HashMap<&str, usize> = graded
            .iter()
            .enumerate()
            .map(|(i, (p, ..))| (p.source_key.as_str(), i))
            .collect();
        let (mut n, mut by_prior, mut by_grade) = (0, 0, 0);
        for c in &checks {
            let (Some(&h), Some(&e)) = (at.get(c.harder.as_str()), at.get(c.easier.as_str()))
            else {
                continue;
            };
            n += 1;
            by_prior += usize::from(prior[h] > prior[e]);
            by_grade += usize::from(grade[h] > grade[e]);
        }
        println!(
            "\nHand-checked pairs ({}): {n} usable; prior agrees on {by_prior}, grade on {by_grade}",
            checks_path.display()
        );
    }
    println!();
}
