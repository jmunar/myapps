use crate::command::{CommandAction, CommandParam, CommandResult, ParamType};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub async fn create_input(
    pool: &SqlitePool,
    user_id: i64,
    classroom_id: i64,
    form_type_id: i64,
    name: &str,
    csv_data: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO classroom_inputs (user_id, classroom_id, form_type_id, name, csv_data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(classroom_id)
    .bind(form_type_id)
    .bind(name)
    .bind(csv_data)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a classroom. Associated inputs are removed by ON DELETE CASCADE.
pub async fn delete_classroom(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM classroom_classrooms WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a form type. Associated inputs are removed by ON DELETE CASCADE.
pub async fn delete_form_type(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM classroom_form_types WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Lookup helpers ──────────────────────────────────────────

pub async fn find_classroom_by_label(
    pool: &SqlitePool,
    user_id: i64,
    label: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM classroom_classrooms WHERE user_id = ? AND LOWER(label) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(label)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn find_form_type_by_name(
    pool: &SqlitePool,
    user_id: i64,
    name: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM classroom_form_types WHERE user_id = ? AND LOWER(name) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn list_classrooms(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT label FROM classroom_classrooms WHERE user_id = ? ORDER BY label")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn list_form_types(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM classroom_form_types WHERE user_id = ? ORDER BY name")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

// ── Command integration ─────────────────────────────────────

static TAB_PARAMS: &[CommandParam] = &[CommandParam {
    name: "tab",
    description: "Page tab: inputs, classrooms, or form-types",
    param_type: ParamType::Text,
    required: false,
}];

static NEW_INPUT_PARAMS: &[CommandParam] = &[
    CommandParam {
        name: "classroom",
        description: "Classroom label",
        param_type: ParamType::Text,
        required: true,
    },
    CommandParam {
        name: "form_type",
        description: "Form type name",
        param_type: ParamType::Text,
        required: true,
    },
];

static CLASSROOM_PARAM: &[CommandParam] = &[CommandParam {
    name: "classroom",
    description: "Classroom label",
    param_type: ParamType::Text,
    required: true,
}];

static FORM_TYPE_PARAM: &[CommandParam] = &[CommandParam {
    name: "name",
    description: "Form type name",
    param_type: ParamType::Text,
    required: true,
}];

pub fn commands() -> Vec<CommandAction> {
    vec![
        CommandAction {
            app: "classroom_input",
            name: "navigate",
            description: "Go to a ClassroomInput page",
            params: TAB_PARAMS,
        },
        CommandAction {
            app: "classroom_input",
            name: "new_input",
            description: "Start a new input for a classroom and form type",
            params: NEW_INPUT_PARAMS,
        },
        CommandAction {
            app: "classroom_input",
            name: "delete_classroom",
            description: "Delete a classroom and its inputs",
            params: CLASSROOM_PARAM,
        },
        CommandAction {
            app: "classroom_input",
            name: "delete_form_type",
            description: "Delete a form type and its inputs",
            params: FORM_TYPE_PARAM,
        },
    ]
}

/// Provide dynamic context for the LLM prompt (available classrooms/form types).
pub async fn command_context(pool: &SqlitePool, user_id: i64) -> HashMap<String, String> {
    let mut ctx = HashMap::new();

    if let Ok(names) = list_classrooms(pool, user_id).await
        && !names.is_empty()
    {
        let val = format!("Available classrooms: {}", names.join(", "));
        ctx.insert("classroom_input.new_input".to_string(), val.clone());
        ctx.insert("classroom_input.delete_classroom".to_string(), val);
    }
    if let Ok(names) = list_form_types(pool, user_id).await
        && !names.is_empty()
    {
        let val = format!("Available form types: {}", names.join(", "));
        // Merge with existing classroom context for new_input
        let new_input_ctx = match ctx.get("classroom_input.new_input") {
            Some(existing) => format!("{existing}. {val}"),
            None => val.clone(),
        };
        ctx.insert("classroom_input.new_input".to_string(), new_input_ctx);
        ctx.insert("classroom_input.delete_form_type".to_string(), val);
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
        "navigate" => {
            let tab = text_param(params, "tab").unwrap_or("");
            let suffix = match tab {
                "classrooms" => "/classrooms",
                "form-types" | "form_types" => "/form-types",
                _ => "",
            };
            Ok(CommandResult::redirect(format!(
                "{base_path}/classroom{suffix}"
            )))
        }

        "new_input" => {
            let classroom = text_param(params, "classroom").ok_or("Missing classroom")?;
            let form_type = text_param(params, "form_type").ok_or("Missing form_type")?;

            let cid = find_classroom_by_label(pool, user_id, classroom)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Classroom '{classroom}' not found."))?;
            let ftid = find_form_type_by_name(pool, user_id, form_type)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Form type '{form_type}' not found."))?;

            Ok(CommandResult::redirect(format!(
                "{base_path}/classroom/new?classroom_id={cid}&form_type_id={ftid}"
            )))
        }

        "delete_classroom" => {
            let label = text_param(params, "classroom").ok_or("Missing classroom")?;
            let id = find_classroom_by_label(pool, user_id, label)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Classroom '{label}' not found."))?;
            delete_classroom(pool, user_id, id).await.map_err(db_err)?;
            Ok(CommandResult::message(format!(
                "Classroom '{label}' deleted."
            )))
        }

        "delete_form_type" => {
            let name = text_param(params, "name").ok_or("Missing name")?;
            let id = find_form_type_by_name(pool, user_id, name)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Form type '{name}' not found."))?;
            delete_form_type(pool, user_id, id).await.map_err(db_err)?;
            Ok(CommandResult::message(format!(
                "Form type '{name}' deleted."
            )))
        }

        _ => Err(format!("Unknown ClassroomInput action: {action}")),
    }
}
