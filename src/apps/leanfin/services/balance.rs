use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use sqlx::SqlitePool;

use crate::config::Config;
use crate::models::Account;

/// On first sync, walk backwards from the current reported balance using daily
/// transaction sums to fill ~90 days of `computed` daily balance rows.
pub async fn backfill_daily_balances(
    pool: &SqlitePool,
    account_id: i64,
    current_balance: f64,
) -> Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let cutoff = (Utc::now() - Duration::days(90)).format("%Y-%m-%d").to_string();

    // Get daily transaction sums ordered newest-first
    let daily_sums: Vec<DailySum> = sqlx::query_as(
        r#"SELECT date, SUM(amount) as total
           FROM transactions
           WHERE account_id = ? AND date >= ?
           GROUP BY date
           ORDER BY date DESC"#,
    )
    .bind(account_id)
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    // Walk backwards: today's balance is known, subtract each day's transactions
    // to get the previous day's closing balance.
    let mut balance = current_balance;
    let mut date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")?;
    let cutoff_date = NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d")?;

    // Build a map of date -> sum for quick lookup
    let mut sum_map = std::collections::HashMap::new();
    for ds in &daily_sums {
        sum_map.insert(ds.date.clone(), ds.total);
    }

    // Insert from today backwards
    while date >= cutoff_date {
        let date_str = date.format("%Y-%m-%d").to_string();
        let source = if date_str == today { "reported" } else { "computed" };

        sqlx::query(
            "INSERT OR IGNORE INTO daily_balances (account_id, date, balance, source) VALUES (?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(&date_str)
        .bind(balance)
        .bind(source)
        .execute(pool)
        .await?;

        // Subtract today's transactions to get yesterday's closing balance
        if let Some(&day_total) = sum_map.get(&date_str) {
            balance -= day_total;
        }

        date -= Duration::days(1);
    }

    tracing::info!("Backfilled daily balances for account {account_id}");
    Ok(())
}

/// Upsert today's balance as a `reported` row.
pub async fn record_daily_balance(
    pool: &SqlitePool,
    account_id: i64,
    balance: f64,
) -> Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();

    sqlx::query(
        r#"INSERT INTO daily_balances (account_id, date, balance, source)
           VALUES (?, ?, ?, 'reported')
           ON CONFLICT(account_id, date) DO UPDATE SET balance = excluded.balance, source = 'reported'"#,
    )
    .bind(account_id)
    .bind(&today)
    .bind(balance)
    .execute(pool)
    .await?;

    Ok(())
}

/// Compare previous reported balance + sum of new transactions vs the new
/// reported balance. If the discrepancy exceeds 0.01, send an ntfy alert.
/// Returns a warning string if there's a mismatch, or None.
pub async fn check_reconciliation(
    pool: &SqlitePool,
    config: &Config,
    account: &Account,
    new_balance: f64,
) -> Result<Option<String>> {
    // Get the most recent reported balance before today
    let today = Utc::now().format("%Y-%m-%d").to_string();

    let prev: Option<PrevBalance> = sqlx::query_as(
        r#"SELECT date, balance FROM daily_balances
           WHERE account_id = ? AND source = 'reported' AND date < ?
           ORDER BY date DESC LIMIT 1"#,
    )
    .bind(account.id)
    .bind(&today)
    .fetch_optional(pool)
    .await?;

    let Some(prev) = prev else {
        // No previous reported balance — nothing to reconcile against
        return Ok(None);
    };

    // Sum transactions between previous date (exclusive) and today (inclusive)
    let txn_sum: Option<f64> = sqlx::query_scalar(
        r#"SELECT SUM(amount) FROM transactions
           WHERE account_id = ? AND date > ? AND date <= ?"#,
    )
    .bind(account.id)
    .bind(&prev.date)
    .bind(&today)
    .fetch_one(pool)
    .await?;

    let txn_sum = txn_sum.unwrap_or(0.0);
    let expected = prev.balance + txn_sum;
    let diff = (expected - new_balance).abs();

    if diff > 0.01 {
        let msg = format!(
            "Balance discrepancy for '{}' ({}): expected {:.2} but got {:.2} (diff: {:.2})",
            account.bank_name,
            account.iban.as_deref().unwrap_or("—"),
            expected,
            new_balance,
            diff,
        );
        tracing::warn!("{msg}");
        crate::services::notify::send(config, &msg).await;
        return Ok(Some(msg));
    }

    Ok(None)
}

/// Query daily balance series for a single account.
pub async fn get_balance_series(
    pool: &SqlitePool,
    account_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let cutoff = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let rows: Vec<BalancePoint> = sqlx::query_as(
        r#"SELECT date, balance, source FROM daily_balances
           WHERE account_id = ? AND date >= ?
           ORDER BY date ASC"#,
    )
    .bind(account_id)
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Query aggregated daily balance series across all accounts for a user.
/// Sums balances per date; source is 'aggregated'.
pub async fn get_aggregated_balance_series(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let cutoff = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let rows: Vec<BalancePoint> = sqlx::query_as(
        r#"SELECT db.date, SUM(db.balance) as balance, 'aggregated' as source
           FROM daily_balances db
           JOIN accounts a ON db.account_id = a.id
           WHERE a.user_id = ? AND db.date >= ?
           GROUP BY db.date
           ORDER BY db.date ASC"#,
    )
    .bind(user_id)
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[derive(sqlx::FromRow)]
struct DailySum {
    date: String,
    total: f64,
}

#[derive(sqlx::FromRow)]
struct PrevBalance {
    date: String,
    balance: f64,
}

#[derive(sqlx::FromRow)]
pub struct BalancePoint {
    pub date: String,
    pub balance: f64,
    pub source: String,
}
