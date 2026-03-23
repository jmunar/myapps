use myapps_core::command::{CommandAction, CommandParam, CommandResult, ParamType};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub async fn create_note(
    pool: &SqlitePool,
    user_id: i64,
    title: &str,
    body: &str,
) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO notes_notes (user_id, title, body) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(user_id)
    .bind(title)
    .bind(body)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn update_note(
    pool: &SqlitePool,
    user_id: i64,
    id: i64,
    title: &str,
    body: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE notes_notes SET title = ?, body = ?, updated_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(title)
    .bind(body)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_note(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM notes_notes WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn toggle_pin(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE notes_notes SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Lookup helpers ──────────────────────────────────────────

pub async fn find_note_by_title(
    pool: &SqlitePool,
    user_id: i64,
    title: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM notes_notes WHERE user_id = ? AND LOWER(title) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(title)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn list_note_titles(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT title FROM notes_notes WHERE user_id = ? ORDER BY updated_at DESC")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

// ── Command integration ─────────────────────────────────────

static NAB_PARAMS: &[CommandParam] = &[];

static NOTE_TITLE_PARAM: &[CommandParam] = &[CommandParam {
    name: "title",
    description: "Note title",
    param_type: ParamType::Text,
    required: true,
}];

pub fn commands() -> Vec<CommandAction> {
    vec![
        CommandAction {
            app: "notes",
            name: "navigate",
            description: "Go to the Notes app",
            params: NAB_PARAMS,
        },
        CommandAction {
            app: "notes",
            name: "new_note",
            description: "Create a new note",
            params: NOTE_TITLE_PARAM,
        },
        CommandAction {
            app: "notes",
            name: "delete_note",
            description: "Delete a note by title",
            params: NOTE_TITLE_PARAM,
        },
    ]
}

pub async fn command_context(pool: &SqlitePool, user_id: i64) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    if let Ok(titles) = list_note_titles(pool, user_id).await
        && !titles.is_empty()
    {
        let val = format!("Available notes: {}", titles.join(", "));
        ctx.insert("notes.delete_note".to_string(), val);
    }
    ctx
}

fn text_param<'a>(params: &'a HashMap<String, serde_json::Value>, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

fn db_err(e: sqlx::Error) -> String {
    format!("Database error: {e}")
}

pub async fn dispatch(
    pool: &SqlitePool,
    user_id: i64,
    action: &str,
    params: &HashMap<String, serde_json::Value>,
    base_path: &str,
) -> Result<CommandResult, String> {
    match action {
        "navigate" => Ok(CommandResult::redirect(format!("{base_path}/notes"))),

        "new_note" => {
            let title = text_param(params, "title").unwrap_or("Untitled");
            let id = create_note(pool, user_id, title, "")
                .await
                .map_err(db_err)?;
            Ok(CommandResult::redirect(format!(
                "{base_path}/notes/{id}/edit"
            )))
        }

        "delete_note" => {
            let title = text_param(params, "title").ok_or("Missing title")?;
            let id = find_note_by_title(pool, user_id, title)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Note '{title}' not found."))?;
            delete_note(pool, user_id, id).await.map_err(db_err)?;
            Ok(CommandResult::message(format!("Note '{title}' deleted.")))
        }

        _ => Err(format!("Unknown Notes action: {action}")),
    }
}
