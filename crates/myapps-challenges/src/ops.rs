use myapps_core::command::{
    CommandAction, CommandParam, CommandResult, ParamType, db_err, text_param,
};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::dataset::Dataset;
use crate::selector::{self, Progress};

/// A second mark for the same problem within this many seconds is taken to be
/// a re-POST (back button, double tap) and ignored.
const DUPLICATE_WINDOW_SECS: i64 = 10;

/// Right answers in a row, since the latest wrong one, that clear a mistake.
pub const REVIEW_RIGHTS_TO_CLEAR: i64 = 2;

/// The cooling-off periods a user can pick, in days; `challenges_settings`
/// checks the same list.
pub const REVIEW_DELAYS: [i64; 4] = [0, 1, 3, 7];

/// The cooling-off for a user with no settings row.
pub const DEFAULT_REVIEW_DELAY: i64 = 3;

/// Whether an attempt is a fresh practice answer or a replay of a mistake.
/// Only practice moves the staircase, feeds the graded draw and counts in
/// the stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Practice,
    Review,
}

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
    pub answer_type: Option<String>,
    pub unit: Option<String>,
    pub source_url: String,
    pub source_title: Option<String>,
    pub license: String,
}

pub async fn get_problem(pool: &SqlitePool, id: i64) -> Result<Option<Problem>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, dataset, subject, topic, difficulty, problem, solution, answer, answer_type,
                unit, source_url, source_title, license
         FROM challenges_problems WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn subjects(pool: &SqlitePool, dataset: Dataset) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT subject FROM challenges_problems
         WHERE dataset = ? AND retired_at IS NULL ORDER BY subject",
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
/// attempted. `exclude` keeps a skipped or just-answered problem from coming
/// straight back: a subject with nothing else in it hands over to the others,
/// in random order, and `exclude` itself is only returned when no subject has
/// anything else. `None` only when the dataset has not been imported.
pub async fn draw(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    exclude: Option<i64>,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    let subjects = subjects(pool, dataset).await?;
    let Some(first) = selector::pick_subject(&subjects, rng) else {
        return Ok(None);
    };
    let mut others: Vec<&str> = subjects
        .iter()
        .map(String::as_str)
        .filter(|s| *s != first)
        .collect();
    others.shuffle(rng);
    for subject in std::iter::once(first).chain(others) {
        if let Some(id) = draw_in_subject(pool, user_id, dataset, subject, exclude, rng).await? {
            return Ok(Some(id));
        }
    }
    Ok(exclude)
}

/// A problem in `subject` other than `exclude`, or `None` if it has no other.
async fn draw_in_subject(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    subject: &str,
    exclude: Option<i64>,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    let level = progress(pool, user_id, dataset, subject).await?.level;
    let target = selector::pick_level(level, dataset.max_level(), rng);
    let exclude = exclude.unwrap_or(-1);

    for difficulty in selector::fallback_order(target, level, dataset.max_level()) {
        if difficulty == dataset.max_level()
            && let Some(id) = draw_graded(pool, user_id, dataset, subject, exclude, rng).await?
        {
            return Ok(Some(id));
        }
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM challenges_problems p
             WHERE dataset = ? AND subject = ? AND difficulty = ? AND id != ?
               AND retired_at IS NULL
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
    sqlx::query_scalar(
        "SELECT p.id FROM challenges_problems p
         JOIN challenges_attempts a ON a.problem_id = p.id AND a.user_id = ?
         WHERE p.dataset = ? AND p.subject = ? AND p.id != ? AND p.retired_at IS NULL
         GROUP BY p.id ORDER BY MAX(a.id) ASC LIMIT 1",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(subject)
    .bind(exclude)
    .fetch_optional(pool)
    .await
}

/// An unseen problem in `subject`'s top level other than `exclude`, chosen by
/// the grader's ranking (see `selector`). `None` when the subject's top level
/// carries no grades, or every graded problem there has been seen; the caller
/// then draws at random as below the top.
async fn draw_graded(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    subject: &str,
    exclude: i64,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    // Every graded problem in the top level, seen or not: percentiles are over
    // the whole level, so a problem keeps its place as you work through it.
    let graded: Vec<(i64, f64, bool)> = sqlx::query_as(
        "SELECT p.id, t.value,
                EXISTS (SELECT 1 FROM challenges_attempts a
                        WHERE a.user_id = ? AND a.problem_id = p.id)
         FROM challenges_problems p
         JOIN challenges_problem_tags t ON t.problem_id = p.id AND t.kind = ?
         WHERE p.dataset = ? AND p.subject = ? AND p.difficulty = ?
           AND p.retired_at IS NULL",
    )
    .bind(user_id)
    .bind(selector::GRADE_TAG)
    .bind(dataset.key())
    .bind(subject)
    .bind(dataset.max_level())
    .fetch_all(pool)
    .await?;
    if graded.is_empty() {
        return Ok(None);
    }
    let grades: Vec<f64> = graded.iter().map(|&(_, g, _)| g).collect();
    let percentile: HashMap<i64, f64> = graded
        .iter()
        .zip(selector::percentiles(&grades))
        .map(|(&(id, _, _), q)| (id, q))
        .collect();

    // Your answers in this top level, oldest first. A problem retired or
    // ungraded since has no percentile and says nothing here.
    let answers: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT a.problem_id, a.correct FROM challenges_attempts a
         JOIN challenges_problems p ON p.id = a.problem_id
         WHERE a.user_id = ? AND p.dataset = ? AND p.subject = ? AND p.difficulty = ?
           AND a.is_review = 0
         ORDER BY a.id",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(subject)
    .bind(dataset.max_level())
    .fetch_all(pool)
    .await?;
    let ability = selector::ability(
        answers
            .iter()
            .filter_map(|(id, right)| percentile.get(id).map(|&q| (q, *right))),
    );

    let unseen: Vec<(i64, f64)> = graded
        .iter()
        .filter(|&&(id, _, seen)| !seen && id != exclude)
        .map(|&(id, _, _)| (id, percentile[&id]))
        .collect();
    Ok(selector::pick_graded(
        &unseen,
        selector::graded_target(ability),
        rng,
    ))
}

/// What marking a problem did to your level in its subject.
pub struct Outcome {
    pub dataset: Dataset,
    pub subject: String,
    pub before: i64,
    pub after: i64,
}

/// Record a self-marked practice attempt and move the staircase, in one
/// transaction. `None` if the problem does not exist.
pub async fn record_attempt(
    pool: &SqlitePool,
    user_id: i64,
    problem_id: i64,
    correct: bool,
) -> Result<Option<Outcome>, sqlx::Error> {
    record(pool, user_id, problem_id, correct, Mode::Practice).await
}

/// Record a replay of a mistake. It counts toward clearing the mistake but
/// leaves the staircase alone, so the outcome's level never changes.
pub async fn record_review(
    pool: &SqlitePool,
    user_id: i64,
    problem_id: i64,
    correct: bool,
) -> Result<Option<Outcome>, sqlx::Error> {
    record(pool, user_id, problem_id, correct, Mode::Review).await
}

async fn record(
    pool: &SqlitePool,
    user_id: i64,
    problem_id: i64,
    correct: bool,
    mode: Mode,
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

    let review = mode == Mode::Review;
    let after = if review {
        before
    } else {
        selector::advance(before, correct, dataset.max_level())
    };

    sqlx::query(
        "INSERT INTO challenges_attempts (user_id, problem_id, correct, level_at, is_review)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(problem_id)
    .bind(correct)
    .bind(before.level)
    .bind(review)
    .execute(&mut *tx)
    .await?;

    if review {
        tx.commit().await?;
        return Ok(Some(Outcome {
            dataset,
            subject,
            before: before.level,
            after: before.level,
        }));
    }

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

// ── Diagrams ────────────────────────────────────────────────

/// The rendered SVG for a diagram hash, if the catalogue has it.
pub async fn diagram_svg(pool: &SqlitePool, hash: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT svg FROM challenges_diagrams WHERE hash = ?")
        .bind(hash)
        .fetch_optional(pool)
        .await
}

/// Which of `hashes` the catalogue has a rendering for.
pub async fn rendered_diagrams(
    pool: &SqlitePool,
    hashes: &[String],
) -> Result<std::collections::HashSet<String>, sqlx::Error> {
    let mut found = std::collections::HashSet::new();
    for hash in hashes {
        let present: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM challenges_diagrams WHERE hash = ?")
                .bind(hash)
                .fetch_optional(pool)
                .await?;
        if present.is_some() {
            found.insert(hash.clone());
        }
    }
    Ok(found)
}

// ── The current problem ────────────────────────────────────

/// The problem you are working on in `dataset`, if any.
pub async fn current(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
) -> Result<Option<i64>, sqlx::Error> {
    // The join drops a row whose problem has been retired or moved dataset,
    // so it is replaced by a fresh draw rather than shown under the wrong
    // heading or kept after the bundle dropped it.
    sqlx::query_scalar(
        "SELECT c.problem_id FROM challenges_current c
         JOIN challenges_problems p ON p.id = c.problem_id AND p.dataset = c.dataset
                                   AND p.retired_at IS NULL
         WHERE c.user_id = ? AND c.dataset = ?",
    )
    .bind(user_id)
    .bind(dataset.key())
    .fetch_optional(pool)
    .await
}

async fn set_current(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    problem_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO challenges_current (user_id, dataset, problem_id) VALUES (?, ?, ?)
         ON CONFLICT (user_id, dataset) DO UPDATE SET problem_id = excluded.problem_id",
    )
    .bind(user_id)
    .bind(dataset.key())
    .bind(problem_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// The problem you are working on in `dataset`, drawing one if there is none.
/// `None` only when the dataset has not been imported.
pub async fn current_or_draw(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    if let Some(id) = current(pool, user_id, dataset).await? {
        return Ok(Some(id));
    }
    let Some(id) = draw(pool, user_id, dataset, None, rng).await? else {
        return Ok(None);
    };
    set_current(pool, user_id, dataset, id).await?;
    Ok(Some(id))
}

/// Swap the current problem for another without recording anything.
pub async fn skip(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    rng: &mut (impl Rng + Send),
) -> Result<Option<i64>, sqlx::Error> {
    let exclude = current(pool, user_id, dataset).await?;
    let Some(id) = draw(pool, user_id, dataset, exclude, rng).await? else {
        return Ok(None);
    };
    set_current(pool, user_id, dataset, id).await?;
    Ok(Some(id))
}

/// Mark the current problem in `dataset` and draw the next one. `None` when
/// `problem_id` is not the current problem — a form from an older page, or a
/// second tap after the first already moved on — and nothing is recorded.
pub async fn mark(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    problem_id: i64,
    correct: bool,
    rng: &mut (impl Rng + Send),
) -> Result<Option<Outcome>, sqlx::Error> {
    if current(pool, user_id, dataset).await? != Some(problem_id) {
        return Ok(None);
    }
    let Some(outcome) = record_attempt(pool, user_id, problem_id, correct).await? else {
        return Ok(None);
    };
    if let Some(next) = draw(pool, user_id, dataset, Some(problem_id), rng).await? {
        set_current(pool, user_id, dataset, next).await?;
    }
    Ok(Some(outcome))
}

// ── Review ──────────────────────────────────────────────────

/// Every mistake of user `?1`, as `state(problem_id, rights, wrong_at,
/// last_at)`: a problem with a wrong attempt, the right answers since its
/// latest wrong one, when that was, and when it was last attempted at all.
/// Nothing about mistakes is stored, so this is the one definition the queue,
/// the counts and the badge all read. Every attempt after the latest wrong one
/// is right by definition, so the count since it, less one, is the rights.
const MISTAKES: &str = "WITH wrong AS (
        SELECT problem_id, MAX(id) AS last_wrong FROM challenges_attempts
        WHERE user_id = ?1 AND correct = 0
        GROUP BY problem_id
    ),
    state AS (
        SELECT w.problem_id, COUNT(a.id) - 1 AS rights,
               MIN(a.created_at) AS wrong_at, MAX(a.created_at) AS last_at
        FROM wrong w
        JOIN challenges_attempts a
          ON a.user_id = ?1 AND a.problem_id = w.problem_id AND a.id >= w.last_wrong
        GROUP BY w.problem_id
    )";

/// SQLite's modifier for "`days` ago", for `datetime('now', ?)`.
fn days_ago(days: i64) -> String {
    format!("-{days} days")
}

/// Your cooling-off period, in days.
pub async fn review_delay(pool: &SqlitePool, user_id: i64) -> Result<i64, sqlx::Error> {
    let days: Option<i64> =
        sqlx::query_scalar("SELECT review_delay_days FROM challenges_settings WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(days.unwrap_or(DEFAULT_REVIEW_DELAY))
}

/// Set your cooling-off period. `false`, and nothing stored, unless `days` is
/// one of `REVIEW_DELAYS`.
pub async fn set_review_delay(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
) -> Result<bool, sqlx::Error> {
    if !REVIEW_DELAYS.contains(&days) {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO challenges_settings (user_id, review_delay_days) VALUES (?, ?)
         ON CONFLICT (user_id) DO UPDATE SET review_delay_days = excluded.review_delay_days",
    )
    .bind(user_id)
    .bind(days)
    .execute(pool)
    .await?;
    Ok(true)
}

/// Where one problem stands as a mistake: `None` if you never got it wrong.
pub struct MistakeState {
    pub rights: i64,
    /// `YYYY-MM-DD` of the latest wrong answer.
    pub wrong_on: String,
    /// `YYYY-MM-DD` it comes back on under the current cooling-off; only
    /// meaningful while it is not cleared.
    pub due_on: String,
}

impl MistakeState {
    pub fn cleared(&self) -> bool {
        self.rights >= REVIEW_RIGHTS_TO_CLEAR
    }
}

pub async fn mistake_state(
    pool: &SqlitePool,
    user_id: i64,
    problem_id: i64,
    delay: i64,
) -> Result<Option<MistakeState>, sqlx::Error> {
    let row: Option<(i64, String, String)> = sqlx::query_as(&format!(
        "{MISTAKES}
         SELECT rights, date(wrong_at), date(last_at, ?3) FROM state WHERE problem_id = ?2"
    ))
    .bind(user_id)
    .bind(problem_id)
    .bind(format!("+{delay} days"))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(rights, wrong_on, due_on)| MistakeState {
        rights,
        wrong_on,
        due_on,
    }))
}

/// The oldest due mistake in `dataset` other than `exclude`. A mistake is due
/// once it is not cleared and its latest attempt is `delay` days old.
pub async fn next_mistake(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    delay: i64,
    exclude: Option<i64>,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "{MISTAKES}
         SELECT s.problem_id FROM state s
         JOIN challenges_problems p ON p.id = s.problem_id
         WHERE s.rights < ?2 AND p.dataset = ?3 AND p.retired_at IS NULL AND p.id != ?4
           AND s.last_at <= datetime('now', ?5)
         ORDER BY s.last_at ASC, s.problem_id ASC
         LIMIT 1"
    ))
    .bind(user_id)
    .bind(REVIEW_RIGHTS_TO_CLEAR)
    .bind(dataset.key())
    .bind(exclude.unwrap_or(-1))
    .bind(days_ago(delay))
    .fetch_optional(pool)
    .await
}

/// How your mistakes in one dataset stand.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MistakeCounts {
    pub due: i64,
    pub waiting: i64,
    pub cleared: i64,
    /// `YYYY-MM-DD` the first waiting mistake comes back on.
    pub next_due_on: Option<String>,
}

pub async fn mistake_counts(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    delay: i64,
) -> Result<MistakeCounts, sqlx::Error> {
    let (due, waiting, cleared, next_due_on): (i64, i64, i64, Option<String>) =
        sqlx::query_as(&format!(
            "{MISTAKES}
             SELECT
                 COALESCE(SUM(s.rights < ?2 AND s.last_at <= datetime('now', ?4)), 0),
                 COALESCE(SUM(s.rights < ?2 AND s.last_at > datetime('now', ?4)), 0),
                 COALESCE(SUM(s.rights >= ?2), 0),
                 date(MIN(CASE WHEN s.rights < ?2 AND s.last_at > datetime('now', ?4)
                               THEN s.last_at END), ?5)
             FROM state s JOIN challenges_problems p ON p.id = s.problem_id
             WHERE p.dataset = ?3 AND p.retired_at IS NULL"
        ))
        .bind(user_id)
        .bind(REVIEW_RIGHTS_TO_CLEAR)
        .bind(dataset.key())
        .bind(days_ago(delay))
        .bind(format!("+{delay} days"))
        .fetch_one(pool)
        .await?;
    Ok(MistakeCounts {
        due,
        waiting,
        cleared,
        next_due_on,
    })
}

/// Due mistakes across the datasets you have not hidden: the Review badge.
pub async fn due_count(pool: &SqlitePool, user_id: i64) -> Result<i64, sqlx::Error> {
    let delay = review_delay(pool, user_id).await?;
    sqlx::query_scalar(&format!(
        "{MISTAKES}
         SELECT COUNT(*) FROM state s JOIN challenges_problems p ON p.id = s.problem_id
         WHERE s.rights < ?2 AND p.retired_at IS NULL
           AND s.last_at <= datetime('now', ?3)
           AND p.dataset NOT IN (SELECT dataset FROM challenges_hidden_datasets
                                 WHERE user_id = ?1)"
    ))
    .bind(user_id)
    .bind(REVIEW_RIGHTS_TO_CLEAR)
    .bind(days_ago(delay))
    .fetch_one(pool)
    .await
}

/// The mistake you are replaying in `dataset`, if it is still due. One
/// cleared in practice meanwhile, or no longer due because you answered it
/// there, or retired, drops out and the next due one takes its place.
pub async fn review_current(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    delay: i64,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "{MISTAKES}
         SELECT c.problem_id FROM challenges_review_current c
         JOIN challenges_problems p ON p.id = c.problem_id AND p.dataset = c.dataset
                                   AND p.retired_at IS NULL
         JOIN state s ON s.problem_id = c.problem_id
         WHERE c.user_id = ?1 AND c.dataset = ?3 AND s.rights < ?2
           AND s.last_at <= datetime('now', ?4)"
    ))
    .bind(user_id)
    .bind(REVIEW_RIGHTS_TO_CLEAR)
    .bind(dataset.key())
    .bind(days_ago(delay))
    .fetch_optional(pool)
    .await
}

async fn set_review_current(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    problem_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    match problem_id {
        Some(id) => sqlx::query(
            "INSERT INTO challenges_review_current (user_id, dataset, problem_id)
             VALUES (?, ?, ?)
             ON CONFLICT (user_id, dataset) DO UPDATE SET problem_id = excluded.problem_id",
        )
        .bind(user_id)
        .bind(dataset.key())
        .bind(id),
        None => {
            sqlx::query("DELETE FROM challenges_review_current WHERE user_id = ? AND dataset = ?")
                .bind(user_id)
                .bind(dataset.key())
        }
    }
    .execute(pool)
    .await?;
    Ok(())
}

/// The mistake you are replaying in `dataset`, taking the oldest due one if
/// there is none. `None` when nothing is due.
pub async fn review_current_or_next(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
) -> Result<Option<i64>, sqlx::Error> {
    let delay = review_delay(pool, user_id).await?;
    if let Some(id) = review_current(pool, user_id, dataset, delay).await? {
        return Ok(Some(id));
    }
    let next = next_mistake(pool, user_id, dataset, delay, None).await?;
    set_review_current(pool, user_id, dataset, next).await?;
    Ok(next)
}

/// "Not now": move on to the next due mistake without recording anything.
/// The only one due stays.
pub async fn review_skip(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
) -> Result<(), sqlx::Error> {
    let delay = review_delay(pool, user_id).await?;
    let current = review_current(pool, user_id, dataset, delay).await?;
    if let Some(next) = next_mistake(pool, user_id, dataset, delay, current).await? {
        set_review_current(pool, user_id, dataset, Some(next)).await?;
    }
    Ok(())
}

/// What replaying a mistake did to it.
pub struct ReviewOutcome {
    pub correct: bool,
    pub state: MistakeState,
}

/// Mark the mistake you are replaying in `dataset` and move to the next due
/// one, never back to this one: with nothing else due, the review is done
/// for now. `None` when `problem_id` is not the one being replayed (a stale
/// form or a second tap), and nothing is recorded.
pub async fn review_mark(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    problem_id: i64,
    correct: bool,
) -> Result<Option<ReviewOutcome>, sqlx::Error> {
    let delay = review_delay(pool, user_id).await?;
    if review_current(pool, user_id, dataset, delay).await? != Some(problem_id) {
        return Ok(None);
    }
    if record_review(pool, user_id, problem_id, correct)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let next = next_mistake(pool, user_id, dataset, delay, Some(problem_id)).await?;
    set_review_current(pool, user_id, dataset, next).await?;
    Ok(mistake_state(pool, user_id, problem_id, delay)
        .await?
        .map(|state| ReviewOutcome { correct, state }))
}

// ── Hidden datasets ─────────────────────────────────────────

pub async fn hidden_datasets(pool: &SqlitePool, user_id: i64) -> Result<Vec<Dataset>, sqlx::Error> {
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT dataset FROM challenges_hidden_datasets WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(keys.iter().filter_map(|k| Dataset::from_key(k)).collect())
}

pub async fn set_dataset_hidden(
    pool: &SqlitePool,
    user_id: i64,
    dataset: Dataset,
    hidden: bool,
) -> Result<(), sqlx::Error> {
    let sql = if hidden {
        "INSERT OR IGNORE INTO challenges_hidden_datasets (user_id, dataset) VALUES (?, ?)"
    } else {
        "DELETE FROM challenges_hidden_datasets WHERE user_id = ? AND dataset = ?"
    };
    sqlx::query(sql)
        .bind(user_id)
        .bind(dataset.key())
        .execute(pool)
        .await?;
    Ok(())
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
    let problems: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM challenges_problems WHERE dataset = ? AND retired_at IS NULL",
    )
    .bind(dataset.key())
    .fetch_one(pool)
    .await?;

    let by_subject: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT p.subject, COUNT(*), SUM(a.correct) FROM challenges_attempts a
         JOIN challenges_problems p ON p.id = a.problem_id
         WHERE a.user_id = ? AND p.dataset = ? AND a.is_review = 0
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
         WHERE a.user_id = ? AND p.dataset = ? AND a.is_review = 0
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

static REVIEW_PARAM: &[CommandParam] = &[CommandParam {
    name: "dataset",
    description: "Dataset key: ugphysics (physics), hendrycks-math (maths) or data-science. Omit to see every dataset.",
    param_type: ParamType::Text,
    required: false,
}];

pub fn commands() -> Vec<CommandAction> {
    vec![
        CommandAction {
            app: "challenges",
            name: "next_problem",
            description: "Open the practice problem in progress, drawing one if there is none",
            params: DATASET_PARAM,
        },
        CommandAction {
            app: "challenges",
            name: "review_mistakes",
            description: "Replay practice problems you got wrong, once their cooling-off has passed",
            params: REVIEW_PARAM,
        },
        CommandAction {
            app: "challenges",
            name: "stats",
            description: "Show accuracy stats for practice problems",
            params: &[],
        },
    ]
}

pub async fn command_context(pool: &SqlitePool, user_id: i64) -> HashMap<String, String> {
    let datasets = Dataset::ALL
        .iter()
        .map(|d| format!("{} ({})", d.key(), d.name()))
        .collect::<Vec<_>>()
        .join(", ");
    let delay = review_delay(pool, user_id)
        .await
        .unwrap_or(DEFAULT_REVIEW_DELAY);
    let mut due = Vec::new();
    for dataset in Dataset::ALL {
        let n = mistake_counts(pool, user_id, dataset, delay)
            .await
            .map_or(0, |c| c.due);
        due.push(format!("{}: {n}", dataset.key()));
    }
    HashMap::from([
        (
            "challenges.next_problem".to_string(),
            format!("Available datasets: {datasets}"),
        ),
        (
            "challenges.review_mistakes".to_string(),
            format!("Mistakes due for review: {}", due.join(", ")),
        ),
    ])
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
            current_or_draw(pool, user_id, dataset, &mut rng)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("{} has not been imported yet.", dataset.name()))?;
            Ok(CommandResult::redirect(format!(
                "{base_path}/challenges/practice/{}",
                dataset.key()
            )))
        }
        "review_mistakes" => match text_param(params, "dataset") {
            Some(key) => {
                let dataset =
                    Dataset::from_key(key).ok_or_else(|| format!("Unknown dataset '{key}'."))?;
                Ok(CommandResult::redirect(format!(
                    "{base_path}/challenges/review/{}",
                    dataset.key()
                )))
            }
            None => Ok(CommandResult::redirect(format!(
                "{base_path}/challenges/review"
            ))),
        },
        "stats" => Ok(CommandResult::redirect(format!(
            "{base_path}/challenges/stats"
        ))),
        _ => Err(format!("Unknown Challenges action: {action}")),
    }
}
