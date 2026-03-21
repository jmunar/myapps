use crate::command::{CommandAction, CommandParam, CommandResult, ParamType};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub async fn capture_thought(
    pool: &SqlitePool,
    user_id: i64,
    content: &str,
    category_id: Option<i64>,
    parent_thought_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO mindflow_thoughts (user_id, category_id, parent_thought_id, content) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(category_id)
    .bind(parent_thought_id)
    .bind(content)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Command integration ─────────────────────────────────────

static CAPTURE_PARAMS: &[CommandParam] = &[CommandParam {
    name: "content",
    description: "The thought content to capture",
    param_type: ParamType::Text,
    required: true,
}];

pub fn commands() -> Vec<CommandAction> {
    vec![CommandAction {
        app: "mindflow",
        name: "capture_thought",
        description: "Capture a new thought",
        params: CAPTURE_PARAMS,
    }]
}

pub async fn dispatch(
    pool: &SqlitePool,
    user_id: i64,
    action: &str,
    params: &HashMap<String, serde_json::Value>,
    _base_path: &str,
) -> Result<CommandResult, String> {
    match action {
        "capture_thought" => {
            let content = params
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or("Missing content parameter")?;
            capture_thought(pool, user_id, content, None, None)
                .await
                .map_err(|e| format!("Database error: {e}"))?;
            Ok(CommandResult::message(format!(
                "Thought captured: \"{content}\""
            )))
        }
        _ => Err(format!("Unknown MindFlow action: {action}")),
    }
}
