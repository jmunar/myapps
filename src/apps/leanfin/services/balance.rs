use anyhow::Result;
use chrono::{Duration, NaiveDate};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::config::Config;
use crate::models::Account;

/// Upsert today's balance as a `reported` row.
pub async fn record_daily_balance(
    pool: &SqlitePool,
    config: &Config,
    account_id: i64,
    balance: f64,
) -> Result<()> {
    let today = config.today().format("%Y-%m-%d").to_string();

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
    let today = config.today().format("%Y-%m-%d").to_string();

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
/// Used for manual accounts that have sparse reported entries.
pub fn fill_balance_gaps(series: &[BalancePoint], days: i64, config: &Config) -> Vec<BalancePoint> {
    if series.is_empty() {
        return Vec::new();
    }

    let today = config.today();
    let cutoff = today - Duration::days(days);

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

/// Query daily balance series for a single account.
/// For bank accounts, computes balances on the fly from the most recent reported
/// balance and transaction sums. For manual accounts, uses stored reported rows
/// with gap-filling.
pub async fn get_balance_series(
    pool: &SqlitePool,
    config: &Config,
    account_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let today = config.today();
    let cutoff = today - Duration::days(days);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
    let today_str = today.format("%Y-%m-%d").to_string();

    // Check account type
    let account_type: String = sqlx::query_scalar(
        "SELECT account_type FROM accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;

    if account_type == "manual" {
        let rows: Vec<BalancePoint> = sqlx::query_as(
            r#"SELECT date, balance, source FROM daily_balances
               WHERE account_id = ? AND date >= ?
               ORDER BY date ASC"#,
        )
        .bind(account_id)
        .bind(&cutoff_str)
        .fetch_all(pool)
        .await?;
        return Ok(fill_balance_gaps(&rows, days, config));
    }

    // Bank account: find the most recent reported balance at or before today
    let anchor: Option<BalancePoint> = sqlx::query_as(
        r#"SELECT date, balance, source FROM daily_balances
           WHERE account_id = ? AND source = 'reported' AND date <= ?
           ORDER BY date DESC LIMIT 1"#,
    )
    .bind(account_id)
    .bind(&today_str)
    .fetch_optional(pool)
    .await?;

    let Some(anchor) = anchor else {
        return Ok(Vec::new());
    };

    // Fetch daily transaction sums for the range
    let daily_sums: Vec<DailySum> = sqlx::query_as(
        r#"SELECT date, SUM(amount) as total FROM transactions
           WHERE account_id = ? AND date >= ?
           GROUP BY date"#,
    )
    .bind(account_id)
    .bind(&cutoff_str)
    .fetch_all(pool)
    .await?;

    let sum_map: HashMap<String, f64> = daily_sums
        .into_iter()
        .map(|ds| (ds.date, ds.total))
        .collect();

    let anchor_date = NaiveDate::parse_from_str(&anchor.date, "%Y-%m-%d")?;

    // Walk backwards from anchor to cutoff
    let mut backward_points: Vec<BalancePoint> = Vec::new();
    let mut balance = anchor.balance;
    let mut date = anchor_date;

    while date >= cutoff {
        let date_str = date.format("%Y-%m-%d").to_string();
        backward_points.push(BalancePoint {
            date: date_str.clone(),
            balance,
            source: "computed".to_string(),
        });
        if let Some(&day_total) = sum_map.get(&date_str) {
            balance -= day_total;
        }
        date -= Duration::days(1);
    }
    backward_points.reverse();

    // Walk forward from anchor to today
    let mut forward_points: Vec<BalancePoint> = Vec::new();
    if anchor_date < today {
        balance = anchor.balance;
        date = anchor_date + Duration::days(1);
        while date <= today {
            let date_str = date.format("%Y-%m-%d").to_string();
            if let Some(&day_total) = sum_map.get(&date_str) {
                balance += day_total;
            }
            forward_points.push(BalancePoint {
                date: date_str,
                balance,
                source: "computed".to_string(),
            });
            date += Duration::days(1);
        }
    }

    backward_points.extend(forward_points);
    Ok(backward_points)
}

/// Query aggregated daily balance series across all accounts for a user.
/// Fetches per-account series then aggregates by date.
pub async fn get_aggregated_balance_series(
    pool: &SqlitePool,
    config: &Config,
    user_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let account_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM accounts WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut date_totals: HashMap<String, f64> = HashMap::new();

    for (account_id,) in &account_ids {
        let filled = get_balance_series(pool, config, *account_id, days).await?;
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
