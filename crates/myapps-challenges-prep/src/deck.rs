//! The decks kept in this repository (`datasets/<key>.jsonl`), read instead
//! of fetched. `datasets/README.md` has the item schema and the rules an item
//! follows; the ones a program can check are checked here, so a bad edit
//! fails the preparation rather than reaching the server.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use myapps_challenges::bundle::Problem;
use myapps_challenges::dataset::Dataset;

#[derive(Deserialize)]
struct Item {
    id: String,
    track: String,
    topic: String,
    difficulty: i64,
    question: String,
    answer: String,
    bullets: Vec<String>,
    source: Source,
}

#[derive(Deserialize)]
struct Source {
    title: String,
    url: String,
}

pub fn default_path(dataset: Dataset) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("datasets")
        .join(format!("{}.jsonl", dataset.key()))
}

/// A track's name as the server shows it: the subject the selector keeps a
/// level for.
fn subject(track: &str) -> Option<&'static str> {
    match track {
        "ml-fundamentals" => Some("ML fundamentals"),
        "metrics" => Some("Metrics"),
        "statistics" => Some("Statistics"),
        "llm" => Some("LLM"),
        _ => None,
    }
}

pub fn read(dataset: Dataset, path: &Path) -> Result<Vec<Problem>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(dataset, &text).with_context(|| format!("in {}", path.display()))
}

fn parse(dataset: Dataset, text: &str) -> Result<Vec<Problem>> {
    let mut ids = HashSet::new();
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let item: Item = serde_json::from_str(line).with_context(|| format!("line {}", i + 1))?;
        ensure!(
            ids.insert(item.id.clone()),
            "line {}: duplicate id '{}'",
            i + 1,
            item.id
        );
        let id = item.id.clone();
        problems.push(map_item(dataset, item).with_context(|| format!("item '{id}'"))?);
    }
    Ok(problems)
}

/// The answer stays a field of its own, marked as prose so the page shows it
/// as the solution's first line; the bullets become the rest of the solution.
fn map_item(dataset: Dataset, item: Item) -> Result<Problem> {
    let Some(subject) = subject(&item.track) else {
        bail!("unknown track '{}'", item.track);
    };
    ensure!(
        (1..=dataset.max_level()).contains(&item.difficulty),
        "difficulty {} is outside 1..={}",
        item.difficulty,
        dataset.max_level()
    );
    for (field, value) in [
        ("id", &item.id),
        ("topic", &item.topic),
        ("question", &item.question),
        ("answer", &item.answer),
        ("source title", &item.source.title),
    ] {
        ensure!(!value.trim().is_empty(), "empty {field}");
    }
    ensure!(
        (2..=4).contains(&item.bullets.len()),
        "{} bullets, not 2 to 4",
        item.bullets.len()
    );
    ensure!(
        item.bullets.iter().all(|b| !b.trim().is_empty()),
        "an empty bullet"
    );
    ensure!(
        item.source.url.starts_with("https://"),
        "source url '{}' is not https",
        item.source.url
    );

    Ok(Problem {
        source_key: item.id,
        subject: subject.to_string(),
        topic: Some(item.topic),
        difficulty: item.difficulty,
        source_level: dataset.level_name(item.difficulty),
        problem: item.question,
        solution: item
            .bullets
            .iter()
            .map(|b| format!("• {}", b.trim()))
            .collect::<Vec<_>>()
            .join("\n"),
        answer: item.answer,
        answer_type: Some("text".into()),
        unit: None,
        source_url: Some(item.source.url),
        source_title: Some(item.source.title),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(overrides: serde_json::Value) -> String {
        let mut item = json!({
            "id": "met-003", "track": "metrics", "topic": "precision", "difficulty": 1,
            "question": "What does precision measure?",
            "answer": "Of the items flagged positive, the fraction that really are positive.",
            "bullets": ["TP/(TP+FP).", "Penalises false positives."],
            "tags": ["precision"],
            "source": { "title": "Google MLCC", "url": "https://developers.google.com/x" },
        });
        for (k, v) in overrides.as_object().unwrap() {
            item[k] = v.clone();
        }
        item.to_string()
    }

    #[test]
    fn maps_an_item_to_a_text_answer_and_bulleted_solution() {
        let p = parse(Dataset::DataScience, &item(json!({})))
            .unwrap()
            .remove(0);
        assert_eq!(p.source_key, "met-003");
        assert_eq!(p.subject, "Metrics");
        assert_eq!(p.topic.as_deref(), Some("precision"));
        assert_eq!(p.source_level, "Recall");
        assert_eq!(p.solution, "• TP/(TP+FP).\n• Penalises false positives.");
        assert_eq!(p.answer_type.as_deref(), Some("text"));
        assert_eq!(
            p.source_url.as_deref(),
            Some("https://developers.google.com/x")
        );
        assert_eq!(p.source_title.as_deref(), Some("Google MLCC"));
    }

    #[test]
    fn refuses_items_that_break_the_rules() {
        for (bad, why) in [
            (json!({ "track": "sql" }), "unknown track"),
            (json!({ "difficulty": 4 }), "outside"),
            (json!({ "answer": " " }), "empty answer"),
            (json!({ "bullets": ["only one"] }), "1 bullets"),
            (json!({ "bullets": ["a", ""] }), "empty bullet"),
            (
                json!({ "source": { "title": "t", "url": "javascript:alert(1)" } }),
                "not https",
            ),
        ] {
            let err = parse(Dataset::DataScience, &item(bad)).unwrap_err();
            assert!(format!("{err:#}").contains(why), "{why}: {err:#}");
        }
        let twice = format!("{}\n{}", item(json!({})), item(json!({})));
        let err = parse(Dataset::DataScience, &twice).unwrap_err();
        assert!(format!("{err:#}").contains("duplicate id"), "{err:#}");
    }

    /// The committed deck itself, so an edit that breaks a rule fails CI.
    #[test]
    fn the_committed_data_science_deck_reads() {
        let dataset = Dataset::DataScience;
        let problems = read(dataset, &default_path(dataset)).unwrap();
        assert_eq!(problems.len(), 122);
        let subjects: HashSet<&str> = problems.iter().map(|p| p.subject.as_str()).collect();
        assert_eq!(subjects.len(), 4);
        for level in 1..=dataset.max_level() {
            assert!(problems.iter().any(|p| p.difficulty == level), "{level}");
        }
    }
}
