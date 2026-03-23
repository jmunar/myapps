use myapps_core::command::{CommandAction, CommandParam, CommandResult};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub struct JobSummary {
    pub filename: String,
    pub status: String,
}

pub async fn list_recent_jobs(
    pool: &SqlitePool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<JobSummary>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT original_filename, status FROM voice_to_text_jobs WHERE user_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(filename, status)| JobSummary { filename, status })
        .collect())
}

// ── Command integration ─────────────────────────────────────

static LIST_PARAMS: &[CommandParam] = &[];

pub fn commands() -> Vec<CommandAction> {
    vec![CommandAction {
        app: "voice_to_text",
        name: "list_jobs",
        description: "List recent transcription jobs",
        params: LIST_PARAMS,
    }]
}

pub async fn dispatch(
    pool: &SqlitePool,
    user_id: i64,
    action: &str,
    _params: &HashMap<String, serde_json::Value>,
    _base_path: &str,
) -> Result<CommandResult, String> {
    match action {
        "list_jobs" => {
            let jobs = list_recent_jobs(pool, user_id, 5)
                .await
                .map_err(|e| format!("Database error: {e}"))?;

            if jobs.is_empty() {
                return Ok(CommandResult::message("No transcription jobs found."));
            }

            let list: Vec<String> = jobs
                .iter()
                .map(|j| format!("{} ({})", j.filename, j.status))
                .collect();

            Ok(CommandResult::message(format!(
                "Recent jobs: {}",
                list.join(", ")
            )))
        }
        _ => Err(format!("Unknown VoiceToText action: {action}")),
    }
}
