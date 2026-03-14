use anyhow::{Result, bail};
use sqlx::SqlitePool;

pub struct ImportResult {
    pub imported: usize,
    pub skipped: Vec<SkippedRow>,
    pub latest_balance: Option<(String, f64)>,
}

pub struct SkippedRow {
    pub line: usize,
    pub reason: String,
}

const MAX_FILE_SIZE: usize = 1_048_576; // 1 MB

pub async fn import_csv_balances(
    pool: &SqlitePool,
    account_id: i64,
    csv_bytes: &[u8],
) -> Result<ImportResult> {
    if csv_bytes.is_empty() {
        bail!("File is empty");
    }
    if csv_bytes.len() > MAX_FILE_SIZE {
        bail!("File exceeds 1 MB limit");
    }

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_bytes);

    // Find date and value columns (case-insensitive, with aliases)
    let headers = rdr.headers()?.clone();
    let date_col = headers.iter().position(|h| h.eq_ignore_ascii_case("date"));
    let value_col = headers.iter().position(|h| {
        h.eq_ignore_ascii_case("value")
            || h.eq_ignore_ascii_case("balance")
            || h.eq_ignore_ascii_case("amount")
    });

    let date_col = match date_col {
        Some(i) => i,
        None => bail!("Missing required column: date"),
    };
    let value_col = match value_col {
        Some(i) => i,
        None => bail!("Missing required column: value (or balance/amount)"),
    };

    // Phase 1: Parse and validate all rows
    let mut rows: Vec<(String, f64)> = Vec::new();
    let mut errors: Vec<SkippedRow> = Vec::new();

    for (idx, result) in rdr.records().enumerate() {
        let line = idx + 2; // 1-based, header is line 1
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                errors.push(SkippedRow {
                    line,
                    reason: format!("Parse error: {e}"),
                });
                continue;
            }
        };

        let date_str = record.get(date_col).unwrap_or("").trim();
        let value_str = record.get(value_col).unwrap_or("").trim();

        if date_str.is_empty() {
            errors.push(SkippedRow {
                line,
                reason: "Empty date".into(),
            });
            continue;
        }
        if value_str.is_empty() {
            errors.push(SkippedRow {
                line,
                reason: "Empty value".into(),
            });
            continue;
        }

        // Validate date format
        if chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").is_err() {
            errors.push(SkippedRow {
                line,
                reason: format!("Invalid date format: '{date_str}' (expected YYYY-MM-DD)"),
            });
            continue;
        }

        // Validate value
        let value: f64 = match value_str.parse() {
            Ok(v) => v,
            Err(_) => {
                errors.push(SkippedRow {
                    line,
                    reason: format!("Invalid number: '{value_str}'"),
                });
                continue;
            }
        };

        rows.push((date_str.to_string(), value));
    }

    // All-or-nothing: if any errors, reject entire import
    if !errors.is_empty() {
        return Ok(ImportResult {
            imported: 0,
            skipped: errors,
            latest_balance: None,
        });
    }

    if rows.is_empty() {
        bail!("No data rows found in CSV");
    }

    // Phase 2: Write all rows in a single transaction
    let mut tx = pool.begin().await?;

    for (date, value) in &rows {
        let timestamp = format!("{date}T23:59:59Z");
        // Delete existing snapshot for same day, then insert
        sqlx::query(
            "DELETE FROM leanfin_balance_snapshots WHERE account_id = ? AND balance_type = 'MANUAL' AND date = ?",
        )
        .bind(account_id)
        .bind(date)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
               VALUES (?, ?, ?, ?, 'MANUAL')"#,
        )
        .bind(account_id)
        .bind(&timestamp)
        .bind(date)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }

    // Find the most recent date and update account balance
    let latest = rows.iter().max_by(|a, b| a.0.cmp(&b.0));
    if let Some((date, value)) = latest {
        // Only update account balance if this is the latest date overall
        let has_newer: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM leanfin_balance_snapshots WHERE account_id = ? AND date > ?)",
        )
        .bind(account_id)
        .bind(date)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(false);

        if !has_newer {
            sqlx::query("UPDATE leanfin_accounts SET balance_amount = ? WHERE id = ?")
                .bind(value)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;

    let imported = rows.len();
    let latest_balance = latest.map(|(d, v)| (d.clone(), *v));

    Ok(ImportResult {
        imported,
        skipped: Vec::new(),
        latest_balance,
    })
}
