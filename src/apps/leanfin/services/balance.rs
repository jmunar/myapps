use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use sqlx::SqlitePool;
use std::collections::HashMap;

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

/// Fill gaps in a sparse balance series by carrying forward the previous day's value.
/// For bank accounts with daily data this is a no-op; for manual accounts it fills
/// missing dates between recorded values.
pub fn fill_balance_gaps(series: &[BalancePoint], days: i64) -> Vec<BalancePoint> {
    if series.is_empty() {
        return Vec::new();
    }

    let cutoff = (Utc::now() - Duration::days(days))
        .naive_utc()
        .date();
    let today = Utc::now().naive_utc().date();

    // Build a map of date -> BalancePoint
    let mut point_map: HashMap<NaiveDate, &BalancePoint> = HashMap::new();
    for p in series {
        if let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") {
            point_map.insert(d, p);
        }
    }

    // Determine the range: from the later of (cutoff, first data point) to today
    let first_date = series.iter()
        .filter_map(|p| NaiveDate::parse_from_str(&p.date, "%Y-%m-%d").ok())
        .min()
        .unwrap_or(cutoff);
    let start = first_date.max(cutoff);

    let mut result = Vec::new();
    let mut date = start;
    let mut last_balance: Option<f64> = None;

    // Check if there's a balance before `start` to carry forward
    for p in series {
        if let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") {
            if d < start {
                last_balance = Some(p.balance);
            }
        }
    }

    while date <= today {
        let date_str = date.format("%Y-%m-%d").to_string();
        if let Some(point) = point_map.get(&date) {
            last_balance = Some(point.balance);
            result.push(BalancePoint {
                date: date_str,
                balance: point.balance,
                source: point.source.clone(),
            });
        } else if let Some(bal) = last_balance {
            result.push(BalancePoint {
                date: date_str,
                balance: bal,
                source: "carried".to_string(),
            });
        }
        // If no last_balance yet (before first data point), skip this date
        date += Duration::days(1);
    }

    result
}

/// Query daily balance series for a single account, with gap-filling.
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

    Ok(fill_balance_gaps(&rows, days))
}

/// Query aggregated daily balance series across all accounts for a user.
/// Fetches per-account series, fills gaps per account, then aggregates by date.
pub async fn get_aggregated_balance_series(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let cutoff = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    // Get all account IDs for this user
    let account_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM accounts WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    // Fetch and gap-fill per account, then aggregate
    let mut date_totals: HashMap<String, f64> = HashMap::new();

    for (account_id,) in &account_ids {
        let rows: Vec<BalancePoint> = sqlx::query_as(
            r#"SELECT date, balance, source FROM daily_balances
               WHERE account_id = ? AND date >= ?
               ORDER BY date ASC"#,
        )
        .bind(account_id)
        .bind(&cutoff)
        .fetch_all(pool)
        .await?;

        let filled = fill_balance_gaps(&rows, days);
        for point in &filled {
            *date_totals.entry(point.date.clone()).or_insert(0.0) += point.balance;
        }
    }

    let mut result: Vec<BalancePoint> = date_totals
        .into_iter()
        .map(|(date, balance)| BalancePoint {
            date,
            balance,
            source: "aggregated".to_string(),
        })
        .collect();

    result.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(result)
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

#[derive(sqlx::FromRow, Clone)]
pub struct BalancePoint {
    pub date: String,
    pub balance: f64,
    pub source: String,
}
