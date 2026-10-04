use myapps_core::command::{
    CommandAction, CommandParam, CommandResult, ParamType, db_err, text_param,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::dataset::Dataset;
use crate::selector::{self, Progress};

/// A second mark for the same problem within this many seconds is taken to be
/// a re-POST (back button, double tap) and ignored.
const DUPLICATE_WINDOW_SECS: i64 = 10;

#[derive(sqlx::FromRow)]
pub struct Problem {
    pub id: i64,
    pub dataset: String,
    pub subject: String,
    pub topic: Option<String>,
    pub difficulty: i64,
    pub problem: String,
    pub solution: String,
    pub answer: String,
    pub unit: Option<String>,
    pub source_url: String,
    pub license: String,
}

pub async fn get_problem(pool: &SqlitePool, id: i64) -> Result<Option<Problem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, dataset, subject, topic, difficulty, problem, solution, answer, unit,
                source_url, license
         FROM challenges_problems WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn subjects(pool: &SqlitePool, dataset: Dataset) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT subject FROM challenges_problems WHERE dataset = ? ORDER BY subject",
    )
    .bind(dataset.key())
    .fetch_all(pool)
    .await
}

pub async fn progress(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    subject: &str,
) -> Result<Progress, sqlx::Error> {
    let row: Option<(i64, i64, bool)> = sqlx::query_as(
        "SELECT level, streak, fast_start FROM challenges_progress
         WHERE user_id = ? AND dataset = ? AND subject = ?",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(subject)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(level, streak, fast_start)| Progress {
            level,
            streak,
            fast_start,
        })
        .unwrap_or_default())
}

/// Pick the next problem: a uniformly random subject, then a level from the
/// neighbourhood of yours in that subject, preferring problems you have never
/// attempted. `exclude` keeps a skipped problem from coming straight back.
/// `None` only when the dataset has not been imported.
pub async fn draw(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    exclude: Option<i64>,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    let subjects = subjects(pool, dataset).await?;
    let Some(subject) = selector::pick_subject(&subjects, rng) else {
        return Ok(None);
    };
    let level = progress(pool, user_id, dataset, subject).await?.level;
    let target = selector::pick_level(level, dataset.max_level(), rng);
    let exclude = exclude.unwrap_or(-1);

    for difficulty in selector::fallback_order(target, level, dataset.max_level()) {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM challenges_problems p
             WHERE dataset = ? AND subject = ? AND difficulty = ? AND id != ?
               AND NOT EXISTS (SELECT 1 FROM challenges_attempts a
                               WHERE a.user_id = ? AND a.problem_id = p.id)
             ORDER BY random() LIMIT 1",
        )
        .bind(dataset.key())
        .bind(subject)
        .bind(difficulty)
        .bind(exclude)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        if id.is_some() {
            return Ok(id);
        }
    }

    // Every problem in the subject has been seen: repeat the one seen longest ago.
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT p.id FROM challenges_problems p
         JOIN challenges_attempts a ON a.problem_id = p.id AND a.user_id = ?
         WHERE p.dataset = ? AND p.subject = ? AND p.id != ?
         GROUP BY p.id ORDER BY MAX(a.id) ASC LIMIT 1",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(subject)
    .bind(exclude)
    .fetch_optional(pool)
    .await?;
    // A one-problem subject whose only problem was just skipped.
    Ok(id.or((exclude != -1).then_some(exclude)))
}

/// What marking a problem did to your level in its subject.
pub struct Outcome {
    pub dataset: Dataset,
    pub subject: String,
    pub before: i64,
    pub after: i64,
}

/// Record a self-marked attempt and move the staircase, in one transaction.
/// `None` if the problem does not exist.
pub async fn record_attempt(
    pool: &SqlitePool,
    user_id: i64,
    problem_id: i64,
    correct: bool,
) -> Result<Option<Outcome>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let row: Option<(String, String)> =
        sqlx::query_as("SELECT dataset, subject FROM challenges_problems WHERE id = ?")
            .bind(problem_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((dataset_key, subject)) = row else {
        return Ok(None);
    };
    let Some(dataset) = Dataset::from_key(&dataset_key) else {
        return Ok(None);
    };

    let before: Progress = {
        let row: Option<(i64, i64, bool)> = sqlx::query_as(
            "SELECT level, streak, fast_start FROM challenges_progress
             WHERE user_id = ? AND dataset = ? AND subject = ?",
        )
        .bind(user_id)
        .bind(dataset.key())
        .bind(&subject)
        .fetch_optional(&mut *tx)
        .await?;
        row.map(|(level, streak, fast_start)| Progress {
            level,
            streak,
            fast_start,
        })
        .unwrap_or_default()
    };

    let duplicate: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM challenges_attempts
         WHERE user_id = ? AND problem_id = ?
           AND created_at >= datetime('now', ?)",
    )
    .bind(user_id)
    .bind(problem_id)
    .bind(format!("-{DUPLICATE_WINDOW_SECS} seconds"))
    .fetch_optional(&mut *tx)
    .await?;
    if duplicate.is_some() {
        return Ok(Some(Outcome {
            dataset,
            subject,
            before: before.level,
            after: before.level,
        }));
    }

    let after = selector::advance(before, correct, dataset.max_level());

    sqlx::query(
        "INSERT INTO challenges_attempts (user_id, problem_id, correct, level_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(problem_id)
    .bind(correct)
    .bind(before.level)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO challenges_progress (user_id, dataset, subject, level, streak, fast_start)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (user_id, dataset, subject)
         DO UPDATE SET level = excluded.level, streak = excluded.streak,
                       fast_start = excluded.fast_start",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(&subject)
    .bind(after.level)
    .bind(after.streak)
    .bind(after.fast_start)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(Outcome {
        dataset,
        subject,
        before: before.level,
        after: after.level,
    }))
}

/// The dataset of your most recent attempt, so "next problem" continues where
/// you left off.
pub async fn last_dataset(pool: &SqlitePool, user_id: i64) -> Result<Option<Dataset>, sqlx::Error> {
    let key: Option<String> = sqlx::query_scalar(
        "SELECT p.dataset FROM challenges_attempts a
         JOIN challenges_problems p ON p.id = a.problem_id
         WHERE a.user_id = ? ORDER BY a.id DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(key.as_deref().and_then(Dataset::from_key))
}

// ── Stats ───────────────────────────────────────────────────

pub struct Tally {
    pub attempts: i64,
    pub correct: i64,
}

pub struct SubjectStats {
    pub subject: String,
    pub level: i64,
    pub tally: Tally,
}

pub struct DatasetStats {
    pub problems: i64,
    pub subjects: Vec<SubjectStats>,
    /// Indexed by difficulty - 1.
    pub levels: Vec<Tally>,
    pub total: Tally,
}

pub async fn dataset_stats(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
) -> Result<DatasetStats, sqlx::Error> {
    let problems: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM challenges_problems WHERE dataset = ?")
            .bind(dataset.key())
            .fetch_one(pool)
            .await?;

    let by_subject: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT p.subject, COUNT(*), SUM(a.correct) FROM challenges_attempts a
         JOIN challenges_problems p ON p.id = a.problem_id
         WHERE a.user_id = ? AND p.dataset = ?
         GROUP BY p.subject",
    )
    .bind(user_id)
    .bind(dataset.key())
    .fetch_all(pool)
    .await?;

    let levels_by_subject: Vec<(String, i64)> = sqlx::query_as(
        "SELECT subject, level FROM challenges_progress WHERE user_id = ? AND dataset = ?",
    )
    .bind(user_id)
    .bind(dataset.key())
    .fetch_all(pool)
    .await?;

    let by_level: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT p.difficulty, COUNT(*), SUM(a.correct) FROM challenges_attempts a
         JOIN challenges_problems p ON p.id = a.problem_id
         WHERE a.user_id = ? AND p.dataset = ?
         GROUP BY p.difficulty",
    )
    .bind(user_id)
    .bind(dataset.key())
    .fetch_all(pool)
    .await?;

    let subjects = subjects(pool, dataset)
        .await?
        .into_iter()
        .map(|subject| {
            let (attempts, correct) = by_subject
                .iter()
                .find(|(s, ..)| *s == subject)
                .map_or((0, 0), |&(_, a, c)| (a, c));
            let level = levels_by_subject
                .iter()
                .find(|(s, _)| *s == subject)
                .map_or(1, |&(_, l)| l);
            SubjectStats {
                subject,
                level,
                tally: Tally { attempts, correct },
            }
        })
        .collect::<Vec<_>>();

    let levels = (1..=dataset.max_level())
        .map(|d| {
            by_level.iter().find(|(l, ..)| *l == d).map_or(
                Tally {
                    attempts: 0,
                    correct: 0,
                },
                |&(_, attempts, correct)| Tally { attempts, correct },
            )
        })
        .collect();

    let total = Tally {
        attempts: by_subject.iter().map(|(_, a, _)| a).sum(),
        correct: by_subject.iter().map(|(_, _, c)| c).sum(),
    };

    Ok(DatasetStats {
        problems,
        subjects,
        levels,
        total,
    })
}

// ── Command integration ─────────────────────────────────────

static DATASET_PARAM: &[CommandParam] = &[CommandParam {
    name: "dataset",
    description: "Dataset key: ugphysics (physics) or hendrycks-math (maths). Omit to continue the last one.",
    param_type: ParamType::Text,
    required: false,
}];

pub fn commands() -> Vec<CommandAction> {
    vec![
        CommandAction {
            app: "challenges",
            name: "next_problem",
            description: "Draw the next practice problem",
            params: DATASET_PARAM,
        },
        CommandAction {
            app: "challenges",
            name: "stats",
            description: "Show accuracy stats for practice problems",
            params: &[],
        },
    ]
}

pub async fn command_context(_pool: &SqlitePool, _user_id: i64) -> HashMap<String, String> {
    let datasets = Dataset::ALL
        .iter()
        .map(|d| format!("{} ({})", d.key(), d.name()))
        .collect::<Vec<_>>()
        .join(", ");
    HashMap::from([(
        "challenges.next_problem".to_string(),
        format!("Available datasets: {datasets}"),
    )])
}

pub async fn dispatch(
    pool: &SqlitePool,
    user_id: i64,
    action: &str,
    params: &HashMap<String, serde_json::Value>,
    base_path: &str,
) -> Result<CommandResult, String> {
    match action {
        "next_problem" => {
            let dataset = match text_param(params, "dataset") {
                Some(key) => {
                    Dataset::from_key(key).ok_or_else(|| format!("Unknown dataset '{key}'."))?
                }
                None => last_dataset(pool, user_id)
                    .await
                    .map_err(db_err)?
                    .unwrap_or(Dataset::Ugphysics),
            };
            let mut rng = StdRng::from_os_rng();
            let id = draw(pool, user_id, dataset, None, &mut rng)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("{} has not been imported yet.", dataset.name()))?;
            Ok(CommandResult::redirect(format!(
                "{base_path}/challenges/problems/{id}"
            )))
        }
        "stats" => Ok(CommandResult::redirect(format!(
            "{base_path}/challenges/stats"
        ))),
        _ => Err(format!("Unknown Challenges action: {action}")),
    }
}
