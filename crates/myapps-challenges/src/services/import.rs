//! Load a dataset into `challenges_problems` from the Hugging Face
//! datasets-server JSON API: in the background when `serve` starts (`auto`),
//! or on demand with `myapps import --app challenges --dataset <key>`.
//!
//! The JSON API rather than the dataset files: Hendrycks MATH is published as
//! Parquet only, and reading it would mean an Arrow dependency on a 4 GB box.
//! The upsert is keyed on `(dataset, source_key)`, so re-running an import
//! keeps problem ids — and with them the attempt history — stable.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::dataset::Dataset;

const ROWS_URL: &str = "https://datasets-server.huggingface.co/rows";
/// The most the datasets-server returns per request.
const PAGE: usize = 100;
const ATTEMPTS: u32 = 6;
/// The datasets-server answers 429 after roughly fifty back-to-back requests,
/// so pace them. A full import is a one-off; a few minutes is fine.
const PAUSE: Duration = Duration::from_millis(1500);

#[derive(Deserialize)]
struct RowsPage {
    rows: Vec<RowEntry>,
    num_rows_total: usize,
}

#[derive(Deserialize)]
struct RowEntry {
    row_idx: usize,
    row: serde_json::Value,
    #[serde(default)]
    truncated_cells: Vec<String>,
}

/// One row, mapped and ready to upsert.
#[derive(Debug, PartialEq)]
pub struct NewProblem {
    pub source_key: String,
    pub subject: String,
    pub topic: Option<String>,
    pub difficulty: i64,
    pub source_level: String,
    pub problem: String,
    pub solution: String,
    pub answer: String,
    pub answer_type: Option<String>,
    pub unit: Option<String>,
}

/// Why a row was left out, for the summary line.
#[derive(Debug, PartialEq)]
pub enum Skip {
    NoLevel,
    Diagram,
    Malformed,
}

#[derive(Default)]
struct Counts {
    imported: usize,
    no_level: usize,
    diagram: usize,
    malformed: usize,
    truncated: usize,
}

/// Import, one after the other, every dataset that has never finished an
/// import and was not filled with the seed's samples. Spawned by `on_serve`,
/// so the server is up while it runs; a failure is logged and retried on the
/// next start.
pub async fn auto(pool: SqlitePool) {
    for dataset in Dataset::ALL {
        match needs_import(&pool, dataset).await {
            Ok(false) => continue,
            Ok(true) => {}
            Err(e) => {
                tracing::error!("{}: could not check import state: {e:#}", dataset.name());
                continue;
            }
        }
        tracing::info!(
            "{}: not imported yet, importing in the background",
            dataset.name()
        );
        if let Err(e) = run(&pool, dataset.key()).await {
            tracing::error!(
                "{}: background import failed, will retry on next start: {e:#}",
                dataset.name()
            );
        }
    }
}

/// True unless the dataset's import has completed, or the seed put its sample
/// problems there (a demo database, which must not start downloading).
pub async fn needs_import(pool: &SqlitePool, dataset: Dataset) -> Result<bool, sqlx::Error> {
    let completed: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM challenges_imports WHERE dataset = ?")
            .bind(dataset.key())
            .fetch_optional(pool)
            .await?;
    let seeded: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM challenges_problems WHERE dataset = ? AND source_key LIKE 'sample/%' LIMIT 1",
    )
    .bind(dataset.key())
    .fetch_optional(pool)
    .await?;
    Ok(completed.is_none() && seeded.is_none())
}

pub async fn run(pool: &SqlitePool, what: &str) -> Result<()> {
    let Some(dataset) = Dataset::from_key(what) else {
        let valid: Vec<&str> = Dataset::ALL.iter().map(|d| d.key()).collect();
        bail!(
            "Unknown dataset '{what}'; expected one of: {}",
            valid.join(", ")
        );
    };

    let client = reqwest::Client::builder()
        .user_agent(concat!("myapps-challenges/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let mut counts = Counts::default();
    for (config, split) in dataset.sources() {
        let mut offset = 0;
        loop {
            let page = fetch_page(&client, dataset, config, split, offset).await?;
            let mut problems = Vec::with_capacity(page.rows.len());
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
                match map_row(dataset, config, split, entry.row_idx, &entry.row) {
                    Ok(p) => problems.push(p),
                    Err(Skip::NoLevel) => counts.no_level += 1,
                    Err(Skip::Diagram) => counts.diagram += 1,
                    Err(Skip::Malformed) => {
                        tracing::warn!(
                            "{config}/{split} row {}: malformed, skipped",
                            entry.row_idx
                        );
                        counts.malformed += 1;
                    }
                }
            }
            counts.imported += problems.len();
            upsert(pool, dataset, &problems).await?;

            offset += PAGE;
            if offset >= page.num_rows_total {
                break;
            }
        }
        tracing::info!("{}: {config}/{split} done", dataset.name());
    }

    sqlx::query(
        "INSERT INTO challenges_imports (dataset, problems) VALUES (?, ?)
         ON CONFLICT (dataset) DO UPDATE SET problems = excluded.problems,
                                             completed_at = datetime('now')",
    )
    .bind(dataset.key())
    .bind(counts.imported as i64)
    .execute(pool)
    .await?;

    tracing::info!(
        "{}: {} problems imported; skipped {} without a level, {} with a diagram, \
         {} malformed, {} truncated",
        dataset.name(),
        counts.imported,
        counts.no_level,
        counts.diagram,
        counts.malformed,
        counts.truncated,
    );
    Ok(())
}

async fn fetch_page(
    client: &reqwest::Client,
    dataset: Dataset,
    config: &str,
    split: &str,
    offset: usize,
) -> Result<RowsPage> {
    let context = || format!("{config}/{split} @ {offset}");
    let mut backoff = Duration::from_secs(5);
    for attempt in 1..=ATTEMPTS {
        tokio::time::sleep(PAUSE).await;
        let response = client
            .get(ROWS_URL)
            .query(&[
                ("dataset", dataset.hf_id()),
                ("config", config),
                ("split", split),
                ("offset", &offset.to_string()),
                ("length", &PAGE.to_string()),
            ])
            .send()
            .await;
        let retry_in = match response {
            Ok(r) if r.status().is_success() => {
                return r
                    .json()
                    .await
                    .with_context(|| format!("{}: unexpected response", context()));
            }
            Ok(r)
                if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || r.status().is_server_error() =>
            {
                let retry_after = r
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .map(Duration::from_secs);
                tracing::warn!("{}: HTTP {}, retrying", context(), r.status());
                retry_after.unwrap_or(backoff)
            }
            Ok(r) => bail!("{}: HTTP {}", context(), r.status()),
            Err(e) if attempt < ATTEMPTS => {
                tracing::warn!("{}: {e}, retrying", context());
                backoff
            }
            Err(e) => return Err(e).with_context(context),
        };
        tokio::time::sleep(retry_in).await;
        backoff *= 2;
    }
    bail!("{}: still failing after {ATTEMPTS} attempts", context())
}

pub fn map_row(
    dataset: Dataset,
    config: &str,
    split: &str,
    row_idx: usize,
    row: &serde_json::Value,
) -> Result<NewProblem, Skip> {
    let text = |key: &str| row.get(key).and_then(|v| v.as_str()).map(str::trim);
    let non_empty = |key: &str| text(key).filter(|s| !s.is_empty()).map(str::to_string);

    let source_level = text("level").unwrap_or("");
    let difficulty = dataset.difficulty(source_level).ok_or(Skip::NoLevel)?;
    let problem = non_empty("problem").ok_or(Skip::Malformed)?;
    let solution = non_empty("solution").ok_or(Skip::Malformed)?;

    match dataset {
        Dataset::Ugphysics => {
            let index = row.get("index").and_then(|v| v.as_i64());
            Ok(NewProblem {
                source_key: match index {
                    Some(i) => format!("{config}/{i}"),
                    None => format!("{config}/row-{row_idx}"),
                },
                subject: non_empty("subject").unwrap_or_else(|| config.to_string()),
                topic: non_empty("topic"),
                difficulty,
                source_level: source_level.to_string(),
                problem,
                solution,
                answer: non_empty("answers").ok_or(Skip::Malformed)?,
                answer_type: non_empty("answer_type")
                    .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                unit: non_empty("unit"),
            })
        }
        Dataset::HendrycksMath => {
            // Asymptote diagrams cannot be drawn in the browser, and the
            // problem is not answerable without them.
            if problem.contains("[asy]") {
                return Err(Skip::Diagram);
            }
            Ok(NewProblem {
                source_key: format!("{config}/{split}/{row_idx}"),
                subject: non_empty("type").unwrap_or_else(|| config.to_string()),
                topic: None,
                difficulty,
                source_level: source_level.to_string(),
                answer: last_boxed(&solution).unwrap_or_default().to_string(),
                problem,
                solution,
                answer_type: None,
                unit: None,
            })
        }
    }
}

/// The contents of the last `\boxed{…}` (or `\fbox{…}`) in `s`, matching
/// braces rather than using a regex, since answers nest them (`\frac{1}{2}`).
pub fn last_boxed(s: &str) -> Option<&str> {
    let start = ["\\boxed{", "\\fbox{"]
        .iter()
        .filter_map(|m| s.rfind(m).map(|i| i + m.len()))
        .max()?;
    let mut depth = 1;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

async fn upsert(pool: &SqlitePool, dataset: Dataset, problems: &[NewProblem]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for p in problems {
        sqlx::query(
            "INSERT INTO challenges_problems
                 (dataset, source_key, subject, topic, difficulty, source_level, problem,
                  solution, answer, answer_type, unit, source_url, license)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (dataset, source_key) DO UPDATE SET
                 subject = excluded.subject, topic = excluded.topic,
                 difficulty = excluded.difficulty, source_level = excluded.source_level,
                 problem = excluded.problem, solution = excluded.solution,
                 answer = excluded.answer, answer_type = excluded.answer_type,
                 unit = excluded.unit, source_url = excluded.source_url,
                 license = excluded.license, imported_at = datetime('now')",
        )
        .bind(dataset.key())
        .bind(&p.source_key)
        .bind(&p.subject)
        .bind(&p.topic)
        .bind(p.difficulty)
        .bind(&p.source_level)
        .bind(&p.problem)
        .bind(&p.solution)
        .bind(&p.answer)
        .bind(&p.answer_type)
        .bind(&p.unit)
        .bind(dataset.url())
        .bind(dataset.license())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn last_boxed_matches_nested_braces() {
        assert_eq!(
            last_boxed(r"so $x = \boxed{\frac{1}{2}}$."),
            Some(r"\frac{1}{2}")
        );
        assert_eq!(last_boxed(r"\boxed{1} then \boxed{2}"), Some("2"));
        assert_eq!(last_boxed(r"\fbox{7}"), Some("7"));
        assert_eq!(last_boxed("no answer"), None);
        assert_eq!(last_boxed(r"\boxed{unclosed"), None);
    }

    #[test]
    fn maps_a_ugphysics_row() {
        let row = json!({
            "index": 1523, "subject": "Classical Mechanics", "topic": "Particle Dynamics",
            "problem": "A rocket…", "solution": "Solve…", "answers": "\\boxed{v}",
            "answer_type": "EX", "unit": null, "level": "Math Derivation",
        });
        let p = map_row(Dataset::Ugphysics, "ClassicalMechanics", "en", 0, &row).unwrap();
        assert_eq!(p.source_key, "ClassicalMechanics/1523");
        assert_eq!(p.subject, "Classical Mechanics");
        assert_eq!(p.difficulty, 3);
        assert_eq!(p.answer, "\\boxed{v}");
        assert_eq!(p.unit, None);
    }

    #[test]
    fn keeps_only_the_first_line_of_a_garbled_answer_type() {
        let row = json!({
            "index": 1, "subject": "S", "problem": "p", "solution": "s", "answers": "a",
            "answer_type": "NV\n   \nThe final answer is…", "level": "Laws Application",
        });
        let p = map_row(Dataset::Ugphysics, "C", "en", 0, &row).unwrap();
        assert_eq!(p.answer_type.as_deref(), Some("NV"));
    }

    #[test]
    fn skips_ugphysics_rows_without_a_level() {
        let row =
            json!({ "index": 1, "problem": "p", "solution": "s", "answers": "a", "level": "" });
        assert_eq!(
            map_row(Dataset::Ugphysics, "C", "en", 0, &row),
            Err(Skip::NoLevel)
        );
    }

    #[test]
    fn maps_a_hendrycks_row_and_extracts_the_answer() {
        let row = json!({
            "problem": "Find x.", "level": "Level 4", "type": "Algebra",
            "solution": "Thus $x = \\boxed{(18, -18)}$.",
        });
        let p = map_row(Dataset::HendrycksMath, "algebra", "test", 12, &row).unwrap();
        assert_eq!(p.source_key, "algebra/test/12");
        assert_eq!(p.subject, "Algebra");
        assert_eq!(p.difficulty, 4);
        assert_eq!(p.answer, "(18, -18)");
    }

    #[test]
    fn skips_hendrycks_diagrams_and_unknown_levels() {
        let diagram = json!({
            "problem": "[asy]draw((0,0)--(1,1));[/asy] Find the area.",
            "level": "Level 2", "type": "Geometry", "solution": "\\boxed{1}",
        });
        assert_eq!(
            map_row(Dataset::HendrycksMath, "geometry", "train", 0, &diagram),
            Err(Skip::Diagram)
        );
        let unknown = json!({
            "problem": "p", "level": "Level ?", "type": "Geometry", "solution": "s",
        });
        assert_eq!(
            map_row(Dataset::HendrycksMath, "geometry", "train", 0, &unknown),
            Err(Skip::NoLevel)
        );
    }
}
