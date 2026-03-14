use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::config::Config;
use crate::models::Account;

/// Determine the timestamp to store for a given balance type.
/// - ITAV, XPCD, ITBD → now (intraday snapshots)
/// - CLAV, CLBD → close of business ({reference_date}T23:59:59Z or today)
pub fn timestamp_for_balance_type(balance_type: &str, reference_date: Option<&str>) -> String {
    match balance_type {
        "CLAV" | "CLBD" => {
            let date = reference_date.unwrap_or_else(|| {
                // Leak is fine for a short-lived string; avoids lifetime issues.
                // In practice reference_date is almost always Some.
                Box::leak(Utc::now().format("%Y-%m-%d").to_string().into_boxed_str())
            });
            format!("{date}T23:59:59Z")
        }
        _ => Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }
}

/// Extract the date portion (YYYY-MM-DD) from a full ISO 8601 timestamp.
pub fn date_from_timestamp(ts: &str) -> &str {
    if ts.len() >= 10 { &ts[..10] } else { ts }
}

/// Insert (or replace same-day duplicate) a balance snapshot.
pub async fn record_balance_snapshot(
    pool: &SqlitePool,
    account_id: i64,
    balance: f64,
    balance_type: &str,
    timestamp: &str,
) -> Result<i64> {
    let date = date_from_timestamp(timestamp);

    // Remove existing snapshot for same account + type + day, then insert.
    sqlx::query(
        "DELETE FROM leanfin_balance_snapshots WHERE account_id = ? AND balance_type = ? AND date = ?",
    )
    .bind(account_id)
    .bind(balance_type)
    .bind(date)
    .execute(pool)
    .await?;

    let result = sqlx::query(
        r#"INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(account_id)
    .bind(timestamp)
    .bind(date)
    .bind(balance)
    .bind(balance_type)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Compare previous ITAV balance + new transactions (linked to this snapshot)
/// vs the new ITAV balance. If the discrepancy exceeds 0.01, send a push notification.
/// Only runs when both old and new balances are ITAV.
pub async fn check_reconciliation(
    pool: &SqlitePool,
    config: &Config,
    account: &Account,
    new_balance: f64,
    balance_type: &str,
    snapshot_id: Option<i64>,
) -> Result<Option<String>> {
    if balance_type != "ITAV" {
        return Ok(None);
    }

    let today = Utc::now().format("%Y-%m-%d").to_string();

    let prev: Option<PrevBalance> = sqlx::query_as(
        r#"SELECT date, balance FROM leanfin_balance_snapshots
           WHERE account_id = ? AND balance_type = 'ITAV' AND date < ?
           ORDER BY date DESC LIMIT 1"#,
    )
    .bind(account.id)
    .bind(&today)
    .fetch_optional(pool)
    .await?;

    let Some(prev) = prev else {
        return Ok(None);
    };

    // Sum only the transactions that are new in this snapshot
    let txn_sum: Option<f64> = if let Some(sid) = snapshot_id {
        sqlx::query_scalar(
            "SELECT SUM(amount) FROM leanfin_transactions WHERE snapshot_id = ?",
        )
        .bind(sid)
        .fetch_one(pool)
        .await?
    } else {
        // Fallback: date-based (legacy, when snapshot_id is unavailable)
        sqlx::query_scalar(
            r#"SELECT SUM(amount) FROM leanfin_transactions
               WHERE account_id = ? AND date > ? AND date <= ?"#,
        )
        .bind(account.id)
        .bind(&prev.date)
        .bind(&today)
        .fetch_one(pool)
        .await?
    };

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
        crate::services::notify::send(pool, config, "LeanFin", &msg).await;
        return Ok(Some(msg));
    }

    Ok(None)
}

/// Fill gaps in a sparse balance series by carrying forward the previous day's value.
/// Used for manual accounts that have sparse reported entries.
pub fn fill_balance_gaps(series: &[BalancePoint], days: i64) -> Vec<BalancePoint> {
    if series.is_empty() {
        return Vec::new();
    }

    let cutoff = (Utc::now() - Duration::days(days))
        .naive_utc()
        .date();
    let today = Utc::now().naive_utc().date();

    let mut point_map: HashMap<NaiveDate, &BalancePoint> = HashMap::new();
    for p in series {
        if let Ok(d) = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d") {
            point_map.insert(d, p);
        }
    }

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
            });
        } else if let Some(bal) = last_balance {
            result.push(BalancePoint {
                date: date_str,
                balance: bal,
            });
        }
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
    account_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let cutoff = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();
    let today = Utc::now().format("%Y-%m-%d").to_string();

    let account_type: String = sqlx::query_scalar(
        "SELECT account_type FROM leanfin_accounts WHERE id = ?",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;

    if account_type == "manual" {
        let rows: Vec<BalancePoint> = sqlx::query_as(
            r#"SELECT date, balance FROM leanfin_balance_snapshots
               WHERE account_id = ? AND date >= ?
               ORDER BY date ASC"#,
        )
        .bind(account_id)
        .bind(&cutoff)
        .fetch_all(pool)
        .await?;
        return Ok(fill_balance_gaps(&rows, days));
    }

    // Bank account: fetch all snapshots in range, ordered by date
    let snapshots: Vec<Snapshot> = sqlx::query_as(
        r#"SELECT id, date, balance FROM leanfin_balance_snapshots
           WHERE account_id = ? AND date >= ?
           ORDER BY date ASC"#,
    )
    .bind(account_id)
    .bind(&cutoff)
    .fetch_all(pool)
    .await?;

    // Also fetch the most recent snapshot before cutoff (for backward walk)
    let pre_cutoff: Option<Snapshot> = sqlx::query_as(
        r#"SELECT id, date, balance FROM leanfin_balance_snapshots
           WHERE account_id = ? AND date < ?
           ORDER BY date DESC LIMIT 1"#,
    )
    .bind(account_id)
    .bind(&cutoff)
    .fetch_optional(pool)
    .await?;

    let all_snapshots: Vec<&Snapshot> = pre_cutoff.iter().chain(snapshots.iter()).collect();

    if all_snapshots.is_empty() {
        return Ok(Vec::new());
    }

    let cutoff_date = NaiveDate::parse_from_str(&cutoff, "%Y-%m-%d")?;
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")?;

    let mut result: Vec<BalancePoint> = Vec::new();

    // Walk backwards from the first snapshot using its linked transactions.
    // This covers the initial sync case where all historical transactions
    // are linked to the first (and only) snapshot.
    let first = all_snapshots[0];
    let first_date = NaiveDate::parse_from_str(&first.date, "%Y-%m-%d")?;
    if first_date > cutoff_date {
        let backward_sums: Vec<DailySum> = sqlx::query_as(
            r#"SELECT date, SUM(amount) as total FROM leanfin_transactions
               WHERE snapshot_id = ? AND date < ?
               GROUP BY date"#,
        )
        .bind(first.id)
        .bind(&first.date)
        .fetch_all(pool)
        .await?;

        if !backward_sums.is_empty() {
            let bw_map: HashMap<String, f64> = backward_sums
                .into_iter()
                .map(|ds| (ds.date, ds.total))
                .collect();

            let mut balance = first.balance;
            let mut date = first_date;
            // Walk backwards: subtract each day's transactions to get prior balance
            while date > cutoff_date {
                let date_str = date.format("%Y-%m-%d").to_string();
                if let Some(&day_total) = bw_map.get(&date_str) {
                    balance -= day_total;
                }
                date -= Duration::days(1);
                let prev_str = date.format("%Y-%m-%d").to_string();
                if date >= cutoff_date {
                    result.push(BalancePoint {
                        date: prev_str,
                        balance,
                    });
                }
            }
        }
    }

    // For each pair of consecutive snapshots, interpolate using linked transactions
    for i in 0..all_snapshots.len() {
        let s = all_snapshots[i];
        let s_date = NaiveDate::parse_from_str(&s.date, "%Y-%m-%d")?;

        // Determine the end date for this segment
        let end_date = if i + 1 < all_snapshots.len() {
            let next = all_snapshots[i + 1];
            NaiveDate::parse_from_str(&next.date, "%Y-%m-%d")?
        } else {
            today_date
        };

        // Fetch daily sums of NEW transactions in the next snapshot (if any)
        let next_snapshot_id = if i + 1 < all_snapshots.len() {
            Some(all_snapshots[i + 1].id)
        } else {
            None
        };

        let sum_map: HashMap<String, f64> = if let Some(nsid) = next_snapshot_id {
            let sums: Vec<DailySum> = sqlx::query_as(
                r#"SELECT date, SUM(amount) as total FROM leanfin_transactions
                   WHERE snapshot_id = ?
                   GROUP BY date"#,
            )
            .bind(nsid)
            .fetch_all(pool)
            .await?;
            sums.into_iter().map(|ds| (ds.date, ds.total)).collect()
        } else {
            // After the last snapshot, use all transactions without a snapshot
            // that fall after this snapshot's date (legacy/future data)
            let sums: Vec<DailySum> = sqlx::query_as(
                r#"SELECT date, SUM(amount) as total FROM leanfin_transactions
                   WHERE account_id = ? AND date > ? AND snapshot_id IS NULL
                   GROUP BY date"#,
            )
            .bind(account_id)
            .bind(&s.date)
            .fetch_all(pool)
            .await?;
            sums.into_iter().map(|ds| (ds.date, ds.total)).collect()
        };

        // Walk from s_date to end_date (exclusive of end_date if there's a next snapshot)
        let mut balance = s.balance;
        let mut date = s_date;
        let walk_end = if next_snapshot_id.is_some() { end_date } else { end_date + Duration::days(1) };

        while date < walk_end {
            let date_str = date.format("%Y-%m-%d").to_string();
            if date >= cutoff_date && date <= today_date {
                result.push(BalancePoint {
                    date: date_str.clone(),
                    balance,
                });
            }
            // Accumulate transactions for the next day
            let next_date = (date + Duration::days(1)).format("%Y-%m-%d").to_string();
            if let Some(&day_total) = sum_map.get(&next_date) {
                balance += day_total;
            }
            date += Duration::days(1);
        }
    }

    // Deduplicate by date (in case of overlapping segments, keep last)
    let mut seen = HashMap::new();
    for p in &result {
        seen.insert(p.date.clone(), p.balance);
    }
    let mut deduped: Vec<BalancePoint> = seen
        .into_iter()
        .map(|(date, balance)| BalancePoint { date, balance })
        .collect();
    deduped.sort_by(|a, b| a.date.cmp(&b.date));

    Ok(deduped)
}

/// Query aggregated daily balance series across all accounts for a user.
pub async fn get_aggregated_balance_series(
    pool: &SqlitePool,
    user_id: i64,
    days: i64,
) -> Result<Vec<BalancePoint>> {
    let account_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut date_totals: HashMap<String, f64> = HashMap::new();

    for (account_id,) in &account_ids {
        let filled = get_balance_series(pool, *account_id, days).await?;
        for point in &filled {
            *date_totals.entry(point.date.clone()).or_insert(0.0) += point.balance;
        }
    }

    let mut result: Vec<BalancePoint> = date_totals
        .into_iter()
        .map(|(date, balance)| BalancePoint { date, balance })
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

#[derive(sqlx::FromRow)]
struct Snapshot {
    id: i64,
    date: String,
    balance: f64,
}

#[derive(sqlx::FromRow, Clone)]
pub struct BalancePoint {
    pub date: String,
    pub balance: f64,
}
