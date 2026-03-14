use anyhow::Result;
use chrono::Duration;
use sqlx::SqlitePool;

use crate::config::Config;

/// A single data point: one label on one date.
#[derive(sqlx::FromRow)]
pub struct ExpensePoint {
    pub date: String,
    pub label_id: i64,
    pub label_name: String,
    pub label_color: Option<String>,
    pub total: f64,
}

/// Get daily expense totals grouped by label for the given label IDs and period.
pub async fn get_expense_series(
    pool: &SqlitePool,
    config: &Config,
    user_id: i64,
    label_ids: &[i64],
    days: i64,
) -> Result<Vec<ExpensePoint>> {
    if label_ids.is_empty() {
        return Ok(vec![]);
    }

    let cutoff = (config.today() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let placeholders = label_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        r#"SELECT t.date, l.id as label_id, l.name as label_name, l.color as label_color,
                  SUM(al.amount) as total
           FROM allocations al
           JOIN transactions t ON al.transaction_id = t.id
           JOIN labels l ON al.label_id = l.id
           JOIN accounts a ON t.account_id = a.id
           WHERE a.user_id = ?
             AND l.id IN ({placeholders})
             AND t.date >= ?
           GROUP BY t.date, l.id
           ORDER BY t.date ASC, l.name ASC"#
    );

    let mut query = sqlx::query_as::<_, ExpensePoint>(&sql).bind(user_id);
    for lid in label_ids {
        query = query.bind(lid);
    }
    query = query.bind(&cutoff);

    let rows = query.fetch_all(pool).await?;
    Ok(rows)
}
