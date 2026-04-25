use myapps_core::command::{CommandAction, CommandParam, CommandResult, ParamType};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub async fn create_input(
    pool: &SqlitePool,
    user_id: i64,
    row_set_id: Option<i64>,
    form_type_id: i64,
    name: &str,
    csv_data: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(row_set_id)
    .bind(form_type_id)
    .bind(name)
    .bind(csv_data)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a row set. Associated inputs are removed by ON DELETE CASCADE.
pub async fn delete_row_set(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM form_input_row_sets WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a form type. Associated inputs are removed by ON DELETE CASCADE.
pub async fn delete_form_type(pool: &SqlitePool, user_id: i64, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM form_input_form_types WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Lookup helpers ──────────────────────────────────────────

pub async fn find_row_set_by_label(
    pool: &SqlitePool,
    user_id: i64,
    label: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM form_input_row_sets WHERE user_id = ? AND LOWER(label) = LOWER(?) LIMIT 1",
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
        "SELECT id FROM form_input_form_types WHERE user_id = ? AND LOWER(name) = LOWER(?) LIMIT 1",
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

pub async fn list_row_sets(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT label FROM form_input_row_sets WHERE user_id = ? ORDER BY label")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn list_form_types(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM form_input_form_types WHERE user_id = ? ORDER BY name")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

// ── Command integration ─────────────────────────────────────

static TAB_PARAMS: &[CommandParam] = &[CommandParam {
    name: "tab",
    description: "Page tab: inputs, row-sets, or form-types",
    param_type: ParamType::Text,
    required: false,
}];

static NEW_INPUT_PARAMS: &[CommandParam] = &[
    CommandParam {
        name: "row_set",
        description: "Row set label",
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

static ROW_SET_PARAM: &[CommandParam] = &[CommandParam {
    name: "row_set",
    description: "Row set label",
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
            app: "form_input",
            name: "navigate",
            description: "Go to a Forms page",
            params: TAB_PARAMS,
        },
        CommandAction {
            app: "form_input",
            name: "new_input",
            description: "Start a new input for a row set and form type",
            params: NEW_INPUT_PARAMS,
        },
        CommandAction {
            app: "form_input",
            name: "delete_row_set",
            description: "Delete a row set and its inputs",
            params: ROW_SET_PARAM,
        },
        CommandAction {
            app: "form_input",
            name: "delete_form_type",
            description: "Delete a form type and its inputs",
            params: FORM_TYPE_PARAM,
        },
    ]
}

/// Provide dynamic context for the LLM prompt (available row sets/form types).
pub async fn command_context(pool: &SqlitePool, user_id: i64) -> HashMap<String, String> {
    let mut ctx = HashMap::new();

    if let Ok(names) = list_row_sets(pool, user_id).await
        && !names.is_empty()
    {
        let val = format!("Available row sets: {}", names.join(", "));
        ctx.insert("form_input.new_input".to_string(), val.clone());
        ctx.insert("form_input.delete_row_set".to_string(), val);
    }
    if let Ok(names) = list_form_types(pool, user_id).await
        && !names.is_empty()
    {
        let val = format!("Available form types: {}", names.join(", "));
        let new_input_ctx = match ctx.get("form_input.new_input") {
            Some(existing) => format!("{existing}. {val}"),
            None => val.clone(),
        };
        ctx.insert("form_input.new_input".to_string(), new_input_ctx);
        ctx.insert("form_input.delete_form_type".to_string(), val);
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
                "row-sets" | "row_sets" => "/row-sets",
                "form-types" | "form_types" => "/form-types",
                _ => "",
            };
            Ok(CommandResult::redirect(format!(
                "{base_path}/forms{suffix}"
            )))
        }

        "new_input" => {
            let row_set = text_param(params, "row_set").ok_or("Missing row_set")?;
            let form_type = text_param(params, "form_type").ok_or("Missing form_type")?;

            let rsid = find_row_set_by_label(pool, user_id, row_set)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Row set '{row_set}' not found."))?;
            let ftid = find_form_type_by_name(pool, user_id, form_type)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Form type '{form_type}' not found."))?;

            Ok(CommandResult::redirect(format!(
                "{base_path}/forms/new?row_set_id={rsid}&form_type_id={ftid}"
            )))
        }

        "delete_row_set" => {
            let label = text_param(params, "row_set").ok_or("Missing row_set")?;
            let id = find_row_set_by_label(pool, user_id, label)
                .await
                .map_err(db_err)?
                .ok_or_else(|| format!("Row set '{label}' not found."))?;
            delete_row_set(pool, user_id, id).await.map_err(db_err)?;
            Ok(CommandResult::message(format!(
                "Row set '{label}' deleted."
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

        _ => Err(format!("Unknown FormInput action: {action}")),
    }
}
