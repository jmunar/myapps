use anyhow::Result;
use sqlx::SqlitePool;

use crate::models::{LabelRule, Transaction};

/// Run auto-labeling rules against transactions that have no auto-assigned labels.
pub async fn apply_rules(pool: &SqlitePool, user_id: i64) -> Result<u64> {
    let rules: Vec<LabelRule> = sqlx::query_as(
        r#"
        SELECT lr.id, lr.label_id, lr.field, lr.pattern, lr.priority
        FROM label_rules lr
        JOIN labels l ON lr.label_id = l.id
        WHERE l.user_id = ?
        ORDER BY lr.priority DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    if rules.is_empty() {
        return Ok(0);
    }

    // Find transactions without auto-labels
    let unlabeled: Vec<Transaction> = sqlx::query_as(
        r#"
        SELECT t.id, t.account_id, t.external_id, t.date, t.amount,
               t.currency, t.description, t.counterparty, t.balance_after,
               t.created_at
        FROM transactions t
        JOIN accounts a ON t.account_id = a.id
        WHERE a.user_id = ?
          AND t.id NOT IN (
            SELECT transaction_id FROM transaction_labels WHERE source = 'auto'
          )
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut applied = 0u64;

    for tx in &unlabeled {
        for rule in &rules {
            let field_value = match rule.field.as_str() {
                "description" => &tx.description,
                "counterparty" => match &tx.counterparty {
                    Some(v) => v,
                    None => continue,
                },
                _ => continue,
            };

            // Simple case-insensitive substring match for now
            // TODO: support regex and amount_range
            if field_value
                .to_lowercase()
                .contains(&rule.pattern.to_lowercase())
            {
                let result = sqlx::query(
                    "INSERT OR IGNORE INTO transaction_labels (transaction_id, label_id, source) VALUES (?, ?, 'auto')",
                )
                .bind(tx.id)
                .bind(rule.label_id)
                .execute(pool)
                .await;

                if let Ok(r) = result {
                    applied += r.rows_affected();
                }
            }
        }
    }

    Ok(applied)
}
