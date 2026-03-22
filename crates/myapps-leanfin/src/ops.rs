use myapps_core::command::{CommandAction, CommandParam, CommandResult, ParamType};
use sqlx::SqlitePool;
use std::collections::HashMap;

// ── Reusable action functions ───────────────────────────────

pub async fn add_transaction(
    pool: &SqlitePool,
    account_id: i64,
    description: &str,
    amount: f64,
) -> Result<(), sqlx::Error> {
    let external_id = format!("cmd-{}", uuid::Uuid::new_v4());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description) \
         VALUES (?, ?, ?, ?, 'EUR', ?)",
    )
    .bind(account_id)
    .bind(&external_id)
    .bind(&today)
    .bind(amount)
    .bind(description)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find the user's first active manual account, if any.
pub async fn first_manual_account(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<i64>, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE user_id = ? AND account_type = 'manual' AND archived = 0 LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}

// ── Command integration ─────────────────────────────────────

static ADD_TXN_PARAMS: &[CommandParam] = &[
    CommandParam {
        name: "description",
        description: "Transaction description",
        param_type: ParamType::Text,
        required: true,
    },
    CommandParam {
        name: "amount",
        description: "Transaction amount (negative for expenses)",
        param_type: ParamType::Number,
        required: true,
    },
];

pub fn commands() -> Vec<CommandAction> {
    vec![CommandAction {
        app: "leanfin",
        name: "add_transaction",
        description: "Add a manual expense transaction",
        params: ADD_TXN_PARAMS,
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
        "add_transaction" => {
            let description = params
                .get("description")
                .and_then(|v| v.as_str())
                .ok_or("Missing description parameter")?;
            let amount = params
                .get("amount")
                .and_then(|v| v.as_f64())
                .ok_or("Missing amount parameter")?;

            let account_id = first_manual_account(pool, user_id)
                .await
                .map_err(|e| format!("Database error: {e}"))?
                .ok_or("No manual account found. Create a manual account in LeanFin first.")?;

            add_transaction(pool, account_id, description, amount)
                .await
                .map_err(|e| format!("Database error: {e}"))?;

            Ok(CommandResult::message(format!(
                "Transaction added: \"{description}\" ({amount:.2})"
            )))
        }
        _ => Err(format!("Unknown LeanFin action: {action}")),
    }
}
