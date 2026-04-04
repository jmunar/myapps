use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use myapps_core::registry::delete_user_app_data;

/// Format a date relative to today: `days_ago(0)` = today, `days_ago(5)` = 5 days ago.
fn days_ago(n: i64) -> String {
    (Utc::now() - Duration::days(n))
        .format("%Y-%m-%d")
        .to_string()
}

pub async fn run(
    pool: &SqlitePool,
    user_id: i64,
    app: &dyn myapps_core::registry::App,
) -> Result<()> {
    delete_user_app_data(pool, app, user_id).await?;

    let expires_in_6m = (Utc::now() + Duration::days(180))
        .format("%Y-%m-%dT00:00:00Z")
        .to_string();

    // Create two bank accounts
    let acct1 = insert_bank_account(
        pool,
        user_id,
        "Santander",
        "ES",
        Some("ES12 3456 7890 1234 5678 9012"),
        "sess_fake_1",
        "uid_checking_001",
        &expires_in_6m,
    )
    .await?;

    let acct2 = insert_bank_account(
        pool,
        user_id,
        "ING Direct",
        "ES",
        Some("ES98 7654 3210 9876 5432 1098"),
        "sess_fake_2",
        "uid_savings_001",
        &expires_in_6m,
    )
    .await?;

    // Create a manual account (investment portfolio)
    let acct3 =
        insert_manual_account(pool, user_id, "Stock Portfolio", "investment", "EUR").await?;

    // Create an archived bank account (old account, no longer in use)
    let acct4 = insert_bank_account(
        pool,
        user_id,
        "BBVA",
        "ES",
        Some("ES55 1234 0000 6789 0123 4567"),
        "sess_fake_3",
        "uid_old_001",
        &expires_in_6m,
    )
    .await?;
    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ?")
        .bind(acct4)
        .execute(pool)
        .await?;

    tracing::info!("Created 2 bank accounts + 1 manual account + 1 archived bank account");

    // Seed transactions using days-ago offsets (0 = today)
    let txns: Vec<(String, f64, &str, &str, Option<&str>)> = vec![
        // Checking account — daily life
        (days_ago(1), -3.50, "EUR", "Coffee", Some("Starbucks")),
        (days_ago(2), -42.30, "EUR", "Weekly groceries", Some("Mercadona")),
        (days_ago(3), -12.99, "EUR", "Netflix subscription", Some("Netflix")),
        (days_ago(4), -35.00, "EUR", "Fuel", Some("Repsol")),
        (days_ago(5), -8.50, "EUR", "Lunch", Some("Bar El Rincón")),
        (days_ago(6), 2150.00, "EUR", "Salary March", Some("Acme Corp")),
        (days_ago(7), -650.00, "EUR", "Rent March", Some("Inmobiliaria López")),
        (days_ago(8), -29.99, "EUR", "Gym membership", Some("Basic-Fit")),
        (days_ago(9), -15.00, "EUR", "Parking", Some("EasyPark")),
        (days_ago(10), -55.80, "EUR", "Groceries", Some("Carrefour")),
        (days_ago(11), -4.20, "EUR", "Coffee", Some("Starbucks")),
        (days_ago(13), -120.00, "EUR", "Electricity bill", Some("Iberdrola")),
        (days_ago(15), -45.00, "EUR", "Internet + phone", Some("Movistar")),
        (days_ago(16), -22.50, "EUR", "Dinner out", Some("Restaurante La Plaza")),
        (days_ago(17), -9.99, "EUR", "Spotify subscription", Some("Spotify")),
        (days_ago(18), -67.30, "EUR", "Groceries", Some("Mercadona")),
        (days_ago(19), -18.00, "EUR", "Cinema tickets", Some("Yelmo Cines")),
        (days_ago(20), -35.00, "EUR", "Fuel", Some("Cepsa")),
        (days_ago(22), -250.00, "EUR", "Insurance payment", Some("Mapfre")),
        (days_ago(24), -14.50, "EUR", "Pharmacy", Some("Farmacia García")),
        (days_ago(27), -38.90, "EUR", "Clothing", Some("Zara")),
        (days_ago(28), -32.00, "EUR", "Valentine dinner", Some("Restaurante La Estrella")),
        (days_ago(30), -4.50, "EUR", "Coffee", Some("Starbucks")),
        (days_ago(32), -48.20, "EUR", "Groceries", Some("Mercadona")),
        (days_ago(36), 2150.00, "EUR", "Salary February", Some("Acme Corp")),
        (days_ago(37), -650.00, "EUR", "Rent February", Some("Inmobiliaria López")),
        (days_ago(38), -29.99, "EUR", "Gym membership", Some("Basic-Fit")),
        (days_ago(41), -120.00, "EUR", "Electricity bill", Some("Iberdrola")),
        (days_ago(43), -45.00, "EUR", "Internet + phone", Some("Movistar")),
        (days_ago(45), -52.00, "EUR", "Groceries", Some("Carrefour")),
        (days_ago(48), -9.99, "EUR", "Spotify subscription", Some("Spotify")),
        (days_ago(53), -35.00, "EUR", "Fuel", Some("Repsol")),
        (days_ago(58), -12.99, "EUR", "Netflix subscription", Some("Netflix")),
        (days_ago(63), -75.00, "EUR", "Doctor visit copay", Some("Sanitas")),
    ];

    // Convert to borrowed slices for the seeding functions
    let txns_ref: Vec<(&str, f64, &str, &str, Option<&str>)> = txns
        .iter()
        .map(|(d, a, c, desc, cp)| (d.as_str(), *a, *c, *desc, *cp))
        .collect();

    // Seed bank account transactions with snapshot linking.
    let count =
        seed_bank_txns_with_snapshots(pool, acct1, &txns_ref, "seed_chk", 3245.67).await?;
    seed_api_payloads(pool, acct1, &txns_ref, "seed_chk", 3245.67).await?;

    // Savings account — fewer, larger movements
    let savings_txns: Vec<(String, f64, &str, &str, Option<&str>)> = vec![
        (days_ago(6), 500.00, "EUR", "Transfer from checking", Some("Self transfer")),
        (days_ago(36), 500.00, "EUR", "Transfer from checking", Some("Self transfer")),
        (days_ago(58), -2000.00, "EUR", "Investment fund", Some("MyInvestor")),
        (days_ago(67), 500.00, "EUR", "Transfer from checking", Some("Self transfer")),
        (days_ago(71), 0.85, "EUR", "Interest payment", Some("ING Direct")),
    ];
    let savings_ref: Vec<(&str, f64, &str, &str, Option<&str>)> = savings_txns
        .iter()
        .map(|(d, a, c, desc, cp)| (d.as_str(), *a, *c, *desc, *cp))
        .collect();

    let count2 =
        seed_bank_txns_with_snapshots(pool, acct2, &savings_ref, "seed_sav", 8500.85).await?;
    seed_api_payloads(pool, acct2, &savings_ref, "seed_sav", 8500.85).await?;

    // Archived BBVA account — old transactions (closed ~4 months ago)
    let bbva_txns: Vec<(String, f64, &str, &str, Option<&str>)> = vec![
        (days_ago(130), -650.00, "EUR", "Rent November", Some("Inmobiliaria López")),
        (days_ago(133), -38.70, "EUR", "Weekly groceries", Some("Mercadona")),
        (days_ago(138), -12.99, "EUR", "Netflix subscription", Some("Netflix")),
        (days_ago(143), -45.00, "EUR", "Internet + phone", Some("Movistar")),
        (days_ago(148), 2100.00, "EUR", "Salary November", Some("Acme Corp")),
        (days_ago(153), -35.00, "EUR", "Fuel", Some("Repsol")),
        (days_ago(157), -120.00, "EUR", "Electricity bill", Some("Iberdrola")),
        (days_ago(160), -650.00, "EUR", "Rent October", Some("Inmobiliaria López")),
        (days_ago(168), -52.40, "EUR", "Groceries", Some("Carrefour")),
        (days_ago(173), -9.99, "EUR", "Spotify subscription", Some("Spotify")),
        (days_ago(178), 2100.00, "EUR", "Salary October", Some("Acme Corp")),
    ];
    let bbva_ref: Vec<(&str, f64, &str, &str, Option<&str>)> = bbva_txns
        .iter()
        .map(|(d, a, c, desc, cp)| (d.as_str(), *a, *c, *desc, *cp))
        .collect();

    let count3 =
        seed_bank_txns_with_snapshots(pool, acct4, &bbva_ref, "seed_bbva", 585.92).await?;
    seed_api_payloads(pool, acct4, &bbva_ref, "seed_bbva", 585.92).await?;
    let count = count + count2 + count3;

    // Seed labels, rules, and allocations
    seed_labels(pool, user_id).await?;
    let alloc_count = seed_allocations(pool, user_id).await?;

    // Seed manual account value history (sparse updates)
    let manual_balances: Vec<(String, f64)> = vec![
        (days_ago(65), 15000.00),
        (days_ago(50), 15350.00),
        (days_ago(40), 14800.00),
        (days_ago(25), 15600.00),
        (days_ago(12), 16100.00),
        (days_ago(2), 16450.00),
    ];
    let manual_ref: Vec<(&str, f64)> = manual_balances
        .iter()
        .map(|(d, v)| (d.as_str(), *v))
        .collect();
    seed_manual_balances(pool, acct3, &manual_ref).await?;

    tracing::info!("Seeded {count} transactions, {alloc_count} allocations, daily balances");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_bank_account(
    pool: &SqlitePool,
    user_id: i64,
    bank_name: &str,
    country: &str,
    iban: Option<&str>,
    session_id: &str,
    account_uid: &str,
    expires: &str,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, account_type) VALUES (?, ?, ?, ?, ?, ?, ?, 'bank')"
    )
    .bind(user_id).bind(bank_name).bind(country).bind(iban)
    .bind(session_id).bind(account_uid).bind(expires)
    .execute(pool).await?;

    Ok(result.last_insert_rowid())
}

async fn insert_manual_account(
    pool: &SqlitePool,
    user_id: i64,
    name: &str,
    category: &str,
    currency: &str,
) -> Result<i64> {
    let uid = format!("manual_{name}");
    let result = sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type, account_name, asset_category, balance_currency) VALUES (?, ?, '', '', ?, '9999-12-31T00:00:00Z', 'manual', ?, ?, ?)"
    )
    .bind(user_id).bind(name).bind(&uid).bind(name).bind(category).bind(currency)
    .execute(pool).await?;

    Ok(result.last_insert_rowid())
}

async fn seed_labels(pool: &SqlitePool, user_id: i64) -> Result<()> {
    let labels = &[
        ("Groceries", "#4CAF50"),
        ("Subscriptions", "#9C27B0"),
        ("Transport", "#FF9800"),
        ("Housing", "#795548"),
        ("Dining", "#E91E63"),
        ("Health", "#00BCD4"),
        ("Income", "#2196F3"),
        ("Savings", "#607D8B"),
        ("Utilities", "#FF5722"),
        ("Entertainment", "#FFC107"),
    ];

    for (name, color) in labels {
        sqlx::query("INSERT INTO leanfin_labels (user_id, name, color) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(name)
            .bind(color)
            .execute(pool)
            .await?;
    }

    // Add some auto-labeling rules
    let rules: &[(&str, &str, &str)] = &[
        ("Groceries", "counterparty", "Mercadona"),
        ("Groceries", "counterparty", "Carrefour"),
        ("Subscriptions", "counterparty", "Netflix"),
        ("Subscriptions", "counterparty", "Spotify"),
        ("Transport", "description", "Fuel"),
        ("Housing", "description", "Rent"),
        ("Dining", "counterparty", "Starbucks"),
        ("Dining", "description", "Dinner"),
        ("Dining", "description", "Lunch"),
        ("Health", "description", "Pharmacy"),
        ("Health", "description", "Doctor"),
        ("Income", "description", "Salary"),
        ("Savings", "description", "Transfer from checking"),
        ("Utilities", "counterparty", "Iberdrola"),
        ("Utilities", "counterparty", "Movistar"),
        ("Entertainment", "counterparty", "Yelmo Cines"),
    ];

    for (label_name, field, pattern) in rules {
        sqlx::query(
            "INSERT INTO leanfin_label_rules (label_id, field, pattern) \
             SELECT id, ?, ? FROM leanfin_labels WHERE user_id = ? AND name = ?",
        )
        .bind(field)
        .bind(pattern)
        .bind(user_id)
        .bind(label_name)
        .execute(pool)
        .await?;
    }

    tracing::info!("Seeded {} labels with {} rules", labels.len(), rules.len());
    Ok(())
}

async fn seed_allocations(pool: &SqlitePool, user_id: i64) -> Result<u64> {
    // Helper: get label id by name
    async fn label_id(pool: &SqlitePool, user_id: i64, name: &str) -> Option<i64> {
        sqlx::query_as::<_, (i64,)>("SELECT id FROM leanfin_labels WHERE user_id = ? AND name = ?")
            .bind(user_id)
            .bind(name)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .map(|r| r.0)
    }

    // Fetch all transactions for this user
    let txns: Vec<(i64, String, f64, Option<String>)> = sqlx::query_as(
        r#"SELECT t.id, t.description, t.amount, t.counterparty
           FROM leanfin_transactions t
           JOIN leanfin_accounts a ON t.account_id = a.id
           WHERE a.user_id = ?"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut count: u64 = 0;

    for (txn_id, desc, amount, counterparty) in &txns {
        let cp = counterparty.as_deref().unwrap_or("");
        let desc_lower = desc.to_lowercase();
        let abs_amount = amount.abs();

        // Determine label(s) based on description/counterparty
        let allocations: Vec<(&str, f64)> = if desc_lower.contains("groceries") && abs_amount > 30.0
        {
            // Split example: large grocery trip = groceries + dining
            vec![("Groceries", abs_amount - 7.30), ("Dining", 7.30)]
        } else if cp.contains("Mercadona") || cp.contains("Carrefour") {
            vec![("Groceries", abs_amount)]
        } else if cp.contains("Netflix") || cp.contains("Spotify") {
            vec![("Subscriptions", abs_amount)]
        } else if desc_lower.contains("fuel") || desc_lower.contains("parking") {
            vec![("Transport", abs_amount)]
        } else if desc_lower.contains("rent") || desc_lower.contains("insurance") {
            vec![("Housing", abs_amount)]
        } else if cp.contains("Starbucks")
            || desc_lower.contains("lunch")
            || desc_lower.contains("dinner")
            || desc_lower.contains("valentine")
        {
            vec![("Dining", abs_amount)]
        } else if desc_lower.contains("pharmacy") || desc_lower.contains("doctor") {
            vec![("Health", abs_amount)]
        } else if desc_lower.contains("salary") {
            vec![("Income", abs_amount)]
        } else if desc_lower.contains("transfer from") {
            vec![("Savings", abs_amount)]
        } else if cp.contains("Iberdrola")
            || cp.contains("Movistar")
            || desc_lower.contains("electricity")
            || desc_lower.contains("internet")
        {
            vec![("Utilities", abs_amount)]
        } else if cp.contains("Yelmo") || desc_lower.contains("cinema") {
            vec![("Entertainment", abs_amount)]
        } else if desc_lower.contains("gym") {
            vec![("Health", abs_amount)]
        } else if desc_lower.contains("clothing") {
            vec![("Entertainment", abs_amount)]
        } else {
            continue; // Leave some unlabeled
        };

        for (label_name, alloc_amount) in allocations {
            if let Some(lid) = label_id(pool, user_id, label_name).await {
                sqlx::query(
                    "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
                )
                .bind(txn_id)
                .bind(lid)
                .bind(alloc_amount)
                .execute(pool)
                .await?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Seed bank account transactions with realistic snapshot linking.
/// Simulates daily syncs: creates snapshots at regular intervals, computes
/// the running balance backwards from the final balance, and links each
/// transaction to the snapshot of the sync day at or after its booking date.
async fn seed_bank_txns_with_snapshots(
    pool: &SqlitePool,
    account_id: i64,
    txns: &[(&str, f64, &str, &str, Option<&str>)],
    prefix: &str,
    final_balance: f64,
) -> Result<u64> {
    use chrono::NaiveDate;

    // Sort transactions by date ascending
    #[allow(clippy::type_complexity)]
    let mut sorted: Vec<(NaiveDate, usize, f64, &str, &str, &str, Option<&str>)> = txns
        .iter()
        .enumerate()
        .filter_map(|(i, (date, amount, currency, desc, cp))| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .ok()
                .map(|d| (d, i, *amount, *date, *currency, *desc, *cp))
        })
        .collect();
    sorted.sort_by_key(|(d, _, _, _, _, _, _)| *d);

    if sorted.is_empty() {
        return Ok(0);
    }

    let first_date = sorted.first().unwrap().0;
    let today = chrono::Utc::now().naive_utc().date();

    // Create daily snapshot dates from first txn date to today
    let mut snapshot_dates: Vec<NaiveDate> = Vec::new();
    let mut d = first_date;
    while d <= today {
        snapshot_dates.push(d);
        d += chrono::Duration::days(1);
    }

    // Compute balance at each snapshot date by walking backwards from final_balance.
    // final_balance is the balance "now" (after all transactions).
    // Subtract transactions from today backwards to get earlier balances.
    let mut balance_at: std::collections::HashMap<NaiveDate, f64> =
        std::collections::HashMap::new();
    let mut bal = final_balance;
    // Walk from today backwards; at each snapshot date, record the balance,
    // then subtract any transactions on that date (to get the balance before them).
    for &sd in snapshot_dates.iter().rev() {
        balance_at.insert(sd, bal);
        let day_total: f64 = sorted
            .iter()
            .filter(|(txd, _, _, _, _, _, _)| *txd == sd)
            .map(|(_, _, amt, _, _, _, _)| *amt)
            .sum();
        bal -= day_total;
    }

    // Create snapshots and insert transactions linked to them.
    // A transaction is linked to the snapshot of its booking date.
    let mut count = 0u64;
    for &sd in &snapshot_dates {
        let date_str = sd.format("%Y-%m-%d").to_string();
        let timestamp = format!("{date_str}T06:00:00Z"); // simulate 6AM sync
        let snap_balance = balance_at[&sd];

        let snap_id = super::balance::record_balance_snapshot(
            pool,
            account_id,
            snap_balance,
            "ITAV",
            &timestamp,
        )
        .await?;

        // Insert transactions for this date, linked to this snapshot
        for (txd, i, amount, _, currency, desc, cp) in &sorted {
            if *txd == sd {
                let eid = format!("{prefix}_{i:03}");
                let result = sqlx::query(
                    "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, counterparty, snapshot_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
                )
                .bind(account_id).bind(&eid).bind(&date_str)
                .bind(amount).bind(currency).bind(desc).bind(cp)
                .bind(snap_id)
                .execute(pool).await?;
                count += result.rows_affected();
            }
        }
    }

    // Update account's current balance
    sqlx::query(
        "UPDATE leanfin_accounts SET balance_amount = ?, balance_currency = 'EUR' WHERE id = ?",
    )
    .bind(final_balance)
    .bind(account_id)
    .execute(pool)
    .await?;

    Ok(count)
}

/// Seed sparse balance snapshots for a manual account.
async fn seed_manual_balances(
    pool: &SqlitePool,
    account_id: i64,
    values: &[(&str, f64)],
) -> Result<()> {
    let mut last_value = 0.0;
    for (date, value) in values {
        let timestamp = format!("{date}T23:59:59Z");
        sqlx::query(
            r#"INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
               VALUES (?, ?, ?, ?, 'MANUAL')"#,
        )
        .bind(account_id)
        .bind(&timestamp)
        .bind(date)
        .bind(value)
        .execute(pool)
        .await?;
        last_value = *value;
    }

    // Update the current balance on the account
    sqlx::query("UPDATE leanfin_accounts SET balance_amount = ? WHERE id = ?")
        .bind(last_value)
        .bind(account_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Seed fake Enable Banking API payloads so the "More details" viewer has data.
async fn seed_api_payloads(
    pool: &SqlitePool,
    account_id: i64,
    txns: &[(&str, f64, &str, &str, Option<&str>)],
    prefix: &str,
    final_balance: f64,
) -> Result<()> {
    // Build a running balance backwards from final_balance (same logic as snapshot seeding)
    let mut balance_after: Vec<f64> = vec![0.0; txns.len()];
    let mut bal = final_balance;
    // Transactions are provided newest-first; walk forward so the last entry = final_balance
    for (i, (_, amount, _, _, _)) in txns.iter().enumerate() {
        balance_after[i] = bal;
        bal -= amount;
    }

    // Build BankTransaction-shaped JSON objects
    let api_txns: Vec<serde_json::Value> = txns
        .iter()
        .enumerate()
        .map(|(i, (date, amount, currency, desc, cp))| {
            let abs = amount.abs();
            let indicator = if *amount < 0.0 { "DBIT" } else { "CRDT" };
            let eid = format!("{prefix}_{i:03}");

            // Creditor/debtor depends on direction
            let (creditor, debtor) = if *amount < 0.0 {
                (
                    cp.map(|name| serde_json::json!({"name": name})),
                    None::<serde_json::Value>,
                )
            } else {
                (
                    None::<serde_json::Value>,
                    cp.map(|name| serde_json::json!({"name": name})),
                )
            };

            serde_json::json!({
                "transaction_id": eid,
                "entry_reference": format!("REF-{eid}"),
                "booking_date": date,
                "value_date": date,
                "transaction_amount": {
                    "currency": currency,
                    "amount": format!("{abs:.2}")
                },
                "credit_debit_indicator": indicator,
                "status": "booked",
                "creditor": creditor,
                "debtor": debtor,
                "remittance_information": [desc],
                "balance_after_transaction": {
                    "amount": format!("{:.2}", balance_after[i])
                }
            })
        })
        .collect();

    let response_body = serde_json::json!({
        "transactions": api_txns
    });

    sqlx::query(
        r#"INSERT INTO leanfin_api_payloads
           (account_id, method, endpoint, response_body, status_code, duration_ms)
           VALUES (?, 'GET', '/accounts/{uid}/transactions', ?, 200, 342)"#,
    )
    .bind(account_id)
    .bind(response_body.to_string())
    .execute(pool)
    .await?;

    Ok(())
}
