//! Load a prepared bundle (see `crate::bundle`) into `challenges_problems`,
//! with `myapps import --app challenges --dataset <file>`.
//!
//! The bundle is the whole of its dataset: what is in it is upserted, keyed on
//! `(dataset, source_key)` so problem ids, and the attempt history that points
//! at them, survive a reload; what is not is retired, never deleted. The load
//! is one transaction, so a bundle that fails validation changes nothing.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::bundle::{self, Manifest};
use crate::dataset::Dataset;

/// Rows read from the bundle at a time.
const PAGE: i64 = 500;

pub async fn run(pool: &SqlitePool, path: &str) -> Result<()> {
    let bundle = bundle::Reader::open(Path::new(path)).await?;
    let dataset = bundle.dataset;
    let expected = bundle.problem_count().await?;
    ensure!(
        expected > 0,
        "{path} has no problems; loading it would retire all of {}",
        dataset.name()
    );

    let mut tx = pool.begin().await?;

    // Whatever is still in here once every bundle row has been seen is retired.
    let mut unseen: HashSet<String> = sqlx::query_scalar(
        "SELECT source_key FROM challenges_problems WHERE dataset = ? AND retired_at IS NULL",
    )
    .bind(dataset.key())
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();

    let mut loaded = 0;
    let mut after = String::new();
    loop {
        let page = bundle.problems_after(&after, PAGE).await?;
        let Some(last) = page.last() else { break };
        after = last.source_key.clone();
        for p in &page {
            validate(dataset, p).with_context(|| format!("{path}: problem '{}'", p.source_key))?;
            upsert(&mut tx, dataset, &bundle.manifest, p).await?;
            unseen.remove(&p.source_key);
        }
        loaded += page.len() as i64;
    }
    // Paging by key skips nothing but an empty key, which `validate` would
    // have refused; this is the check that it really did read everything.
    ensure!(
        loaded == expected,
        "{path}: read {loaded} of its {expected} problems"
    );

    for key in &unseen {
        sqlx::query(
            "UPDATE challenges_problems SET retired_at = datetime('now')
             WHERE dataset = ? AND source_key = ?",
        )
        .bind(dataset.key())
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "INSERT INTO challenges_imports (dataset, problems, prepared_at, prepared_by)
         VALUES (?, ?, ?, ?)
         ON CONFLICT (dataset) DO UPDATE SET problems = excluded.problems,
                                             prepared_at = excluded.prepared_at,
                                             prepared_by = excluded.prepared_by,
                                             completed_at = datetime('now')",
    )
    .bind(dataset.key())
    .bind(loaded)
    .bind(&bundle.manifest.prepared_at)
    .bind(&bundle.manifest.prepared_by)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        "{}: {loaded} problems loaded, {} retired (bundle prepared {} by {})",
        dataset.name(),
        unseen.len(),
        bundle.manifest.prepared_at,
        bundle.manifest.prepared_by,
    );
    Ok(())
}

/// What the rest of the app assumes of a problem, checked before any of the
/// bundle reaches the table.
fn validate(dataset: Dataset, p: &bundle::Problem) -> Result<()> {
    if !(1..=dataset.max_level()).contains(&p.difficulty) {
        bail!(
            "difficulty {} is outside 1..={}",
            p.difficulty,
            dataset.max_level()
        );
    }
    for (field, value) in [
        ("source_key", &p.source_key),
        ("subject", &p.subject),
        ("problem", &p.problem),
        ("solution", &p.solution),
    ] {
        if value.trim().is_empty() {
            bail!("empty {field}");
        }
    }
    Ok(())
}

async fn upsert(
    tx: &mut Transaction<'_, Sqlite>,
    dataset: Dataset,
    manifest: &Manifest,
    p: &bundle::Problem,
) -> Result<()> {
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
             license = excluded.license, imported_at = datetime('now'),
             retired_at = NULL",
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
    .bind(&manifest.source_url)
    .bind(&manifest.license)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
