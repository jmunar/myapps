//! The grader's output: one JSONL file per dataset, committed under `grades/`,
//! one row per problem with its difficulty estimate and rubric answers. The
//! file is the record — the grader is slow and an LLM, so not reproducible —
//! and this module turns it into the bundle's tags.
//!
//! A row applies to a problem only while the problem's text still hashes to the
//! row's `text_hash`: a Hendrycks `source_key` is a row position, so an
//! upstream reorder or edit must leave a problem ungraded, not mis-graded.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use myapps_challenges::bundle::{Problem, Tag};
use myapps_challenges::dataset::Dataset;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub source_key: String,
    /// `text_hash` of the problem text the grader saw.
    pub text_hash: String,
    /// Tag kind → value, e.g. `grade`, `grade_prior`, one per rubric question.
    pub tags: BTreeMap<String, f64>,
    /// The model and rubric version that produced the row, and the run, so a
    /// file mixing runs says which row came from which.
    pub model: String,
    pub rubric: String,
    pub run: String,
}

/// SHA-256 of a problem's text, exactly as the bundle stores it.
pub fn text_hash(problem: &str) -> String {
    Sha256::digest(problem.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Where the grades for `dataset` live in the repo.
pub fn default_path(dataset: Dataset) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("grades")
        .join(format!("{}-v1.jsonl", dataset.key()))
}

pub fn read(path: &Path) -> Result<Vec<Row>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Row = serde_json::from_str(line)
            .with_context(|| format!("{} line {}", path.display(), n + 1))?;
        if !seen.insert(row.source_key.clone()) {
            bail!(
                "{} line {}: '{}' is graded twice",
                path.display(),
                n + 1,
                row.source_key
            );
        }
        if let Some(kind) = row
            .tags
            .iter()
            .find_map(|(k, v)| (k.trim().is_empty() || !v.is_finite()).then_some(k))
        {
            bail!(
                "{} line {}: tag '{kind}' is empty or not a finite number",
                path.display(),
                n + 1
            );
        }
        rows.push(row);
    }
    Ok(rows)
}

/// The bundle's tags, and how the file's rows matched the problems.
#[derive(Debug, Default)]
pub struct Matched {
    pub tags: Vec<Tag>,
    pub graded: usize,
    /// Rows whose problem's text has changed since it was graded.
    pub stale: usize,
    /// Rows for a problem this preparation does not have.
    pub unknown: usize,
}

pub fn match_problems(problems: &[Problem], rows: &[Row]) -> Matched {
    let by_key: BTreeMap<&str, &Problem> = problems
        .iter()
        .map(|p| (p.source_key.as_str(), p))
        .collect();
    let mut m = Matched::default();
    for row in rows {
        let Some(p) = by_key.get(row.source_key.as_str()) else {
            m.unknown += 1;
            continue;
        };
        if text_hash(&p.problem) != row.text_hash {
            m.stale += 1;
            continue;
        }
        m.graded += 1;
        m.tags.extend(row.tags.iter().map(|(kind, &value)| Tag {
            source_key: row.source_key.clone(),
            kind: kind.clone(),
            value,
        }));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(key: &str, text: &str) -> Problem {
        Problem {
            source_key: key.into(),
            subject: "Algebra".into(),
            topic: None,
            difficulty: 1,
            source_level: "Level 1".into(),
            problem: text.into(),
            solution: "s".into(),
            answer: "a".into(),
            answer_type: None,
            unit: None,
            source_url: None,
            source_title: None,
        }
    }

    fn row(key: &str, text: &str, grade: f64) -> Row {
        Row {
            source_key: key.into(),
            text_hash: text_hash(text),
            tags: BTreeMap::from([("grade".into(), grade), ("steps".into(), 1.5)]),
            model: "m".into(),
            rubric: "r1".into(),
            run: "test".into(),
        }
    }

    #[test]
    fn rows_apply_only_to_the_text_they_graded() {
        let problems = [problem("a", "Solve x"), problem("b", "Solve y, reworded")];
        let rows = [
            row("a", "Solve x", 0.5),
            row("b", "Solve y", 1.0),
            row("gone", "Old", 2.0),
        ];
        let m = match_problems(&problems, &rows);
        assert_eq!((m.graded, m.stale, m.unknown), (1, 1, 1));
        let tags: Vec<_> = m
            .tags
            .iter()
            .map(|t| (t.source_key.as_str(), t.kind.as_str(), t.value))
            .collect();
        assert_eq!(tags, [("a", "grade", 0.5), ("a", "steps", 1.5)]);
    }

    #[test]
    fn the_file_round_trips_and_refuses_duplicates() {
        let dir = std::env::temp_dir().join(format!("grades-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("g.jsonl");
        let rows = [row("a", "x", 0.5), row("b", "y", -1.0)];
        let text: String = rows
            .iter()
            .map(|r| serde_json::to_string(r).unwrap() + "\n")
            .collect();
        std::fs::write(&path, format!("{text}\n")).unwrap();
        assert_eq!(read(&path).unwrap(), rows);

        std::fs::write(&path, format!("{text}{}", text.lines().next().unwrap())).unwrap();
        let err = read(&path).unwrap_err();
        assert!(format!("{err:#}").contains("graded twice"), "{err:#}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_hash_is_of_the_exact_text() {
        assert_eq!(
            text_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(text_hash("abc"), text_hash("abc "));
    }
}
