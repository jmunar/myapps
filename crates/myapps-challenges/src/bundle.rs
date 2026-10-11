//! The prepared-dataset bundle: one SQLite file per dataset, written on a
//! workstation by `myapps-challenges-prep` and loaded on the server by
//! `myapps import --app challenges --dataset <file>`.
//!
//! Everything that is expensive to compute (fetching, cleaning, and later
//! diagrams and extracted features) happens before the bundle exists, so the
//! server only ever copies rows out of it. This module is the contract between
//! the two sides: the schema, the row types, and a writer and a reader that
//! both go through them. A change to the schema bumps `FORMAT`, and the server
//! refuses any bundle whose format it was not built for: anything newer, and
//! anything older than `OLDEST_READABLE`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::dataset::Dataset;

/// The bundle schema version this build writes and reads.
pub const FORMAT: i64 = 2;

/// The oldest format this build still loads. Format 1 is format 2 without the
/// `diagrams` table, so a bundle prepared before diagrams existed (UGPhysics
/// has none) loads as one with no diagrams instead of being prepared again.
pub const OLDEST_READABLE: i64 = 1;

/// The first format with a `diagrams` table.
const DIAGRAMS_SINCE: i64 = 2;

const SCHEMA: &str = "
CREATE TABLE manifest (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    format       INTEGER NOT NULL,
    dataset      TEXT    NOT NULL,   -- Dataset::key()
    source_url   TEXT    NOT NULL,
    license      TEXT    NOT NULL,
    prepared_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    prepared_by  TEXT    NOT NULL    -- the tool and version that wrote it
);

CREATE TABLE problems (
    source_key    TEXT    PRIMARY KEY,  -- stable id within the dataset
    subject       TEXT    NOT NULL,
    topic         TEXT,
    difficulty    INTEGER NOT NULL,     -- 1..=Dataset::max_level()
    source_level  TEXT    NOT NULL,
    problem       TEXT    NOT NULL,
    solution      TEXT    NOT NULL,
    answer        TEXT    NOT NULL,
    answer_type   TEXT,
    unit          TEXT
);

-- Rendered [asy] blocks, by crate::diagram::hash of their source. Every
-- diagram in a problem's text is here; one in a solution may be missing (it
-- failed to render) and is shown as a placeholder.
CREATE TABLE diagrams (
    hash  TEXT PRIMARY KEY,
    svg   TEXT NOT NULL
);
";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Manifest {
    pub format: i64,
    pub dataset: String,
    pub source_url: String,
    pub license: String,
    pub prepared_at: String,
    pub prepared_by: String,
}

/// One problem, as it travels from the preparation to the server.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Problem {
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

/// A rendered diagram.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Diagram {
    pub hash: String,
    pub svg: String,
}

/// Writes a bundle to `<path>.partial` and renames it into place on `finish`,
/// so an interrupted preparation never leaves a file that looks loadable.
pub struct Writer {
    pool: SqlitePool,
    partial: PathBuf,
    path: PathBuf,
}

impl Writer {
    pub async fn create(
        path: &Path,
        dataset: Dataset,
        source_url: &str,
        license: &str,
        prepared_by: &str,
    ) -> Result<Writer> {
        let mut partial = path.as_os_str().to_owned();
        partial.push(".partial");
        let partial = PathBuf::from(partial);
        // A leftover from an interrupted run; it was never a bundle.
        match std::fs::remove_file(&partial) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("removing {}", partial.display()));
            }
            _ => {}
        }

        // A rollback journal rather than WAL: the bundle has to be one
        // self-contained file to copy around and to open read-only.
        let options = SqliteConnectOptions::new()
            .filename(&partial)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .with_context(|| format!("creating {}", partial.display()))?;

        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        sqlx::query(
            "INSERT INTO manifest (id, format, dataset, source_url, license, prepared_by)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(FORMAT)
        .bind(dataset.key())
        .bind(source_url)
        .bind(license)
        .bind(prepared_by)
        .execute(&pool)
        .await?;

        Ok(Writer {
            pool,
            partial,
            path: path.to_path_buf(),
        })
    }

    pub async fn insert(&self, problems: &[Problem]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for p in problems {
            sqlx::query(
                "INSERT INTO problems
                     (source_key, subject, topic, difficulty, source_level, problem,
                      solution, answer, answer_type, unit)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
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
            .execute(&mut *tx)
            .await
            .with_context(|| format!("inserting {}", p.source_key))?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_diagrams(&self, diagrams: &[Diagram]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for d in diagrams {
            sqlx::query("INSERT INTO diagrams (hash, svg) VALUES (?, ?)")
                .bind(&d.hash)
                .bind(&d.svg)
                .execute(&mut *tx)
                .await
                .with_context(|| format!("inserting diagram {}", d.hash))?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn finish(self) -> Result<()> {
        self.pool.close().await;
        std::fs::rename(&self.partial, &self.path)
            .with_context(|| format!("moving the bundle to {}", self.path.display()))
    }
}

/// A bundle opened for loading: read-only, and immutable, so SQLite takes no
/// locks and creates no journal next to a file the server may not own.
pub struct Reader {
    pool: SqlitePool,
    pub manifest: Manifest,
    pub dataset: Dataset,
}

impl Reader {
    pub async fn open(path: &Path) -> Result<Reader> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .immutable(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .with_context(|| format!("opening bundle {}", path.display()))?;

        let manifest: Manifest = sqlx::query_as(
            "SELECT format, dataset, source_url, license, prepared_at, prepared_by
             FROM manifest WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .with_context(|| format!("{} is not a Challenges bundle", path.display()))?;
        if !(OLDEST_READABLE..=FORMAT).contains(&manifest.format) {
            bail!(
                "{} is bundle format {}, but this build reads formats \
                 {OLDEST_READABLE} to {FORMAT}: prepare it again with the matching \
                 myapps-challenges-prep",
                path.display(),
                manifest.format
            );
        }
        let Some(dataset) = Dataset::from_key(&manifest.dataset) else {
            bail!(
                "{} is for unknown dataset '{}'",
                path.display(),
                manifest.dataset
            );
        };

        Ok(Reader {
            pool,
            manifest,
            dataset,
        })
    }

    pub async fn problem_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM problems")
            .fetch_one(&self.pool)
            .await?)
    }

    /// Up to `limit` problems with a key after `after`, in key order: pages
    /// through the bundle without holding all of it in memory.
    pub async fn problems_after(&self, after: &str, limit: i64) -> Result<Vec<Problem>> {
        Ok(sqlx::query_as(
            "SELECT source_key, subject, topic, difficulty, source_level, problem, solution,
                    answer, answer_type, unit
             FROM problems WHERE source_key > ? ORDER BY source_key LIMIT ?",
        )
        .bind(after)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn diagram_count(&self) -> Result<i64> {
        if self.manifest.format < DIAGRAMS_SINCE {
            return Ok(0);
        }
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM diagrams")
            .fetch_one(&self.pool)
            .await?)
    }

    /// Up to `limit` diagrams with a hash after `after`, in hash order.
    pub async fn diagrams_after(&self, after: &str, limit: i64) -> Result<Vec<Diagram>> {
        if self.manifest.format < DIAGRAMS_SINCE {
            return Ok(Vec::new());
        }
        Ok(
            sqlx::query_as("SELECT hash, svg FROM diagrams WHERE hash > ? ORDER BY hash LIMIT ?")
                .bind(after)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?,
        )
    }
}
