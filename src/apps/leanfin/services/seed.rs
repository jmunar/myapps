use anyhow::Result;
use sqlx::SqlitePool;

use crate::auth;

pub async fn run(pool: &SqlitePool, reset: bool) -> Result<()> {
    if reset {
        // Delete the demo user; ON DELETE CASCADE wipes all related data
        let result = sqlx::query("DELETE FROM users WHERE username = 'demo'")
            .execute(pool)
            .await?;
        if result.rows_affected() > 0 {
            tracing::info!("Wiped demo user and all associated data");
        }
    }

    // Create demo user (password: "demo")
    let user_id = match auth::create_user(pool, "demo", "demo").await {
        Ok(id) => {
            tracing::info!("Created demo user (username: demo, password: demo)");
            id
        }
        Err(_) => {
            // User already exists, fetch id
            let row: (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
                .fetch_one(pool)
                .await?;
            tracing::info!("Demo user already exists");
            row.0
        }
    };

    // Create two bank accounts
    let acct1 = insert_bank_account(
        pool, user_id, "Santander", "ES", Some("ES12 3456 7890 1234 5678 9012"),
        "sess_fake_1", "uid_checking_001", "2026-09-01T00:00:00Z",
    ).await?;

    let acct2 = insert_bank_account(
        pool, user_id, "ING Direct", "ES", Some("ES98 7654 3210 9876 5432 1098"),
        "sess_fake_2", "uid_savings_001", "2026-08-15T00:00:00Z",
    ).await?;

    // Create a manual account (investment portfolio)
    let acct3 = insert_manual_account(
        pool, user_id, "Stock Portfolio", "investment", "EUR",
    ).await?;

    // Create an archived bank account (old account, no longer in use)
    let acct4 = insert_bank_account(
        pool, user_id, "BBVA", "ES", Some("ES55 1234 0000 6789 0123 4567"),
        "sess_fake_3", "uid_old_001", "2025-12-01T00:00:00Z",
    ).await?;
    sqlx::query("UPDATE leanfin_accounts SET archived = 1 WHERE id = ?")
        .bind(acct4)
        .execute(pool)
        .await?;

    tracing::info!("Created 2 bank accounts + 1 manual account + 1 archived bank account");

    // Seed transactions for the last ~60 days
    let txns: &[(&str, f64, &str, &str, Option<&str>)] = &[
        // Checking account — daily life
        ("2026-03-11", -3.50,  "EUR", "Coffee",                        Some("Starbucks")),
        ("2026-03-10", -42.30, "EUR", "Weekly groceries",              Some("Mercadona")),
        ("2026-03-09", -12.99, "EUR", "Netflix subscription",          Some("Netflix")),
        ("2026-03-08", -35.00, "EUR", "Fuel",                          Some("Repsol")),
        ("2026-03-07", -8.50,  "EUR", "Lunch",                         Some("Bar El Rincón")),
        ("2026-03-06", 2150.00,"EUR", "Salary March",                  Some("Acme Corp")),
        ("2026-03-05", -650.00,"EUR", "Rent March",                    Some("Inmobiliaria López")),
        ("2026-03-04", -29.99, "EUR", "Gym membership",                Some("Basic-Fit")),
        ("2026-03-03", -15.00, "EUR", "Parking",                       Some("EasyPark")),
        ("2026-03-02", -55.80, "EUR", "Groceries",                     Some("Carrefour")),
        ("2026-03-01", -4.20,  "EUR", "Coffee",                        Some("Starbucks")),
        ("2026-02-28", -120.00,"EUR", "Electricity bill",              Some("Iberdrola")),
        ("2026-02-27", -45.00, "EUR", "Internet + phone",              Some("Movistar")),
        ("2026-02-26", -22.50, "EUR", "Dinner out",                    Some("Restaurante La Plaza")),
        ("2026-02-25", -9.99,  "EUR", "Spotify subscription",          Some("Spotify")),
        ("2026-02-24", -67.30, "EUR", "Groceries",                     Some("Mercadona")),
        ("2026-02-23", -18.00, "EUR", "Cinema tickets",                Some("Yelmo Cines")),
        ("2026-02-22", -35.00, "EUR", "Fuel",                          Some("Cepsa")),
        ("2026-02-20", -250.00,"EUR", "Insurance payment",             Some("Mapfre")),
        ("2026-02-18", -14.50, "EUR", "Pharmacy",                      Some("Farmacia García")),
        ("2026-02-15", -38.90, "EUR", "Clothing",                      Some("Zara")),
        ("2026-02-14", -32.00, "EUR", "Valentine dinner",              Some("Restaurante La Estrella")),
        ("2026-02-12", -4.50,  "EUR", "Coffee",                        Some("Starbucks")),
        ("2026-02-10", -48.20, "EUR", "Groceries",                     Some("Mercadona")),
        ("2026-02-06", 2150.00,"EUR", "Salary February",               Some("Acme Corp")),
        ("2026-02-05", -650.00,"EUR", "Rent February",                 Some("Inmobiliaria López")),
        ("2026-02-04", -29.99, "EUR", "Gym membership",                Some("Basic-Fit")),
        ("2026-02-01", -120.00,"EUR", "Electricity bill",              Some("Iberdrola")),
        ("2026-01-30", -45.00, "EUR", "Internet + phone",              Some("Movistar")),
        ("2026-01-28", -52.00, "EUR", "Groceries",                     Some("Carrefour")),
        ("2026-01-25", -9.99,  "EUR", "Spotify subscription",          Some("Spotify")),
        ("2026-01-20", -35.00, "EUR", "Fuel",                          Some("Repsol")),
        ("2026-01-15", -12.99, "EUR", "Netflix subscription",          Some("Netflix")),
        ("2026-01-10", -75.00, "EUR", "Doctor visit copay",            Some("Sanitas")),
    ];

    // Seed bank account transactions with snapshot linking.
    // Simulate daily syncs: create a snapshot at each "sync date", then link
    // transactions whose booking date falls within that sync window.
    let count = seed_bank_txns_with_snapshots(pool, acct1, txns, "seed_chk", 3245.67).await?;

    // Savings account — fewer, larger movements
    let savings_txns: &[(&str, f64, &str, &str, Option<&str>)] = &[
        ("2026-03-06", 500.00,  "EUR", "Transfer from checking", Some("Self transfer")),
        ("2026-02-06", 500.00,  "EUR", "Transfer from checking", Some("Self transfer")),
        ("2026-01-15", -2000.00,"EUR", "Investment fund",         Some("MyInvestor")),
        ("2026-01-06", 500.00,  "EUR", "Transfer from checking", Some("Self transfer")),
        ("2026-01-02", 0.85,    "EUR", "Interest payment",        Some("ING Direct")),
    ];

    let count2 = seed_bank_txns_with_snapshots(pool, acct2, savings_txns, "seed_sav", 8500.85).await?;

    // Archived BBVA account — old transactions (account closed in late 2025)
    let bbva_txns: &[(&str, f64, &str, &str, Option<&str>)] = &[
        ("2025-11-28", -650.00, "EUR", "Rent November",              Some("Inmobiliaria López")),
        ("2025-11-25", -38.70,  "EUR", "Weekly groceries",           Some("Mercadona")),
        ("2025-11-20", -12.99,  "EUR", "Netflix subscription",       Some("Netflix")),
        ("2025-11-15", -45.00,  "EUR", "Internet + phone",           Some("Movistar")),
        ("2025-11-10", 2100.00, "EUR", "Salary November",            Some("Acme Corp")),
        ("2025-11-05", -35.00,  "EUR", "Fuel",                       Some("Repsol")),
        ("2025-11-01", -120.00, "EUR", "Electricity bill",           Some("Iberdrola")),
        ("2025-10-28", -650.00, "EUR", "Rent October",               Some("Inmobiliaria López")),
        ("2025-10-20", -52.40,  "EUR", "Groceries",                  Some("Carrefour")),
        ("2025-10-15", -9.99,   "EUR", "Spotify subscription",       Some("Spotify")),
        ("2025-10-10", 2100.00, "EUR", "Salary October",             Some("Acme Corp")),
    ];

    let count3 = seed_bank_txns_with_snapshots(pool, acct4, bbva_txns, "seed_bbva", 585.92).await?;
    let count = count + count2 + count3;

    // Seed labels, rules, and allocations
    seed_labels(pool, user_id).await?;
    let alloc_count = seed_allocations(pool, user_id).await?;

    // Seed manual account value history (sparse updates)
    seed_manual_balances(pool, acct3, &[
        ("2026-01-05", 15000.00),
        ("2026-01-20", 15350.00),
        ("2026-02-03", 14800.00),
        ("2026-02-17", 15600.00),
        ("2026-03-01", 16100.00),
        ("2026-03-10", 16450.00),
    ]).await?;

    tracing::info!("Seeded {count} transactions, {alloc_count} allocations, daily balances");
    tracing::info!("Ready! Run `cargo run -- serve` and login with demo / demo");
    Ok(())
}

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
    // Use INSERT OR IGNORE + fetch to be idempotent
    sqlx::query(
        "INSERT OR IGNORE INTO leanfin_accounts (user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, account_type) VALUES (?, ?, ?, ?, ?, ?, ?, 'bank')"
    )
    .bind(user_id).bind(bank_name).bind(country).bind(iban)
    .bind(session_id).bind(account_uid).bind(expires)
    .execute(pool).await?;

    let (id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts WHERE account_uid = ?")
        .bind(account_uid)
        .fetch_one(pool).await?;
    Ok(id)
}

async fn insert_manual_account(
    pool: &SqlitePool,
    user_id: i64,
    name: &str,
    category: &str,
    currency: &str,
) -> Result<i64> {
    let uid = format!("manual_{name}");
    sqlx::query(
        "INSERT OR IGNORE INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type, account_name, asset_category, balance_currency) VALUES (?, ?, '', '', ?, '9999-12-31T00:00:00Z', 'manual', ?, ?, ?)"
    )
    .bind(user_id).bind(name).bind(&uid).bind(name).bind(category).bind(currency)
    .execute(pool).await?;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_uid = ?"
    )
    .bind(&uid)
    .fetch_one(pool).await?;
    Ok(id)
}


async fn seed_labels(pool: &SqlitePool, user_id: i64) -> Result<()> {
    let labels = &[
        ("Groceries",      "#4CAF50"),
        ("Subscriptions",  "#9C27B0"),
        ("Transport",      "#FF9800"),
        ("Housing",        "#795548"),
        ("Dining",         "#E91E63"),
        ("Health",         "#00BCD4"),
        ("Income",         "#2196F3"),
        ("Savings",        "#607D8B"),
        ("Utilities",      "#FF5722"),
        ("Entertainment",  "#FFC107"),
    ];

    for (name, color) in labels {
        sqlx::query("INSERT OR IGNORE INTO leanfin_labels (user_id, name, color) VALUES (?, ?, ?)")
            .bind(user_id).bind(name).bind(color)
            .execute(pool).await?;
    }

    // Add some auto-labeling rules
    let rules: &[(&str, &str, &str)] = &[
        ("Groceries",     "counterparty", "Mercadona"),
        ("Groceries",     "counterparty", "Carrefour"),
        ("Subscriptions", "counterparty", "Netflix"),
        ("Subscriptions", "counterparty", "Spotify"),
        ("Transport",     "description",  "Fuel"),
        ("Housing",       "description",  "Rent"),
        ("Dining",        "counterparty", "Starbucks"),
        ("Dining",        "description",  "Dinner"),
        ("Dining",        "description",  "Lunch"),
        ("Health",        "description",  "Pharmacy"),
        ("Health",        "description",  "Doctor"),
        ("Income",        "description",  "Salary"),
        ("Savings",       "description",  "Transfer from checking"),
        ("Utilities",     "counterparty", "Iberdrola"),
        ("Utilities",     "counterparty", "Movistar"),
        ("Entertainment", "counterparty", "Yelmo Cines"),
    ];

    for (label_name, field, pattern) in rules {
        sqlx::query(
            "INSERT OR IGNORE INTO leanfin_label_rules (label_id, field, pattern) \
             SELECT id, ?, ? FROM leanfin_labels WHERE user_id = ? AND name = ?"
        )
        .bind(field).bind(pattern).bind(user_id).bind(label_name)
        .execute(pool).await?;
    }

    tracing::info!("Seeded {} labels with {} rules", labels.len(), rules.len());
    Ok(())
}

async fn seed_allocations(pool: &SqlitePool, user_id: i64) -> Result<u64> {
    // Helper: get label id by name
    async fn label_id(pool: &SqlitePool, user_id: i64, name: &str) -> Option<i64> {
        sqlx::query_as::<_, (i64,)>("SELECT id FROM leanfin_labels WHERE user_id = ? AND name = ?")
            .bind(user_id).bind(name)
            .fetch_optional(pool).await.ok().flatten().map(|r| r.0)
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
        // Skip if already has allocations
        let existing: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM leanfin_allocations WHERE transaction_id = ?",
        )
        .bind(txn_id)
        .fetch_one(pool)
        .await?;
        if existing.0 > 0 {
            continue;
        }

        let cp = counterparty.as_deref().unwrap_or("");
        let desc_lower = desc.to_lowercase();
        let abs_amount = amount.abs();

        // Determine label(s) based on description/counterparty
        let allocations: Vec<(&str, f64)> = if desc_lower.contains("groceries") && abs_amount > 30.0 {
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
        } else if cp.contains("Starbucks") || desc_lower.contains("lunch")
            || desc_lower.contains("dinner") || desc_lower.contains("valentine")
        {
            vec![("Dining", abs_amount)]
        } else if desc_lower.contains("pharmacy") || desc_lower.contains("doctor") {
            vec![("Health", abs_amount)]
        } else if desc_lower.contains("salary") {
            vec![("Income", abs_amount)]
        } else if desc_lower.contains("transfer from") {
            vec![("Savings", abs_amount)]
        } else if cp.contains("Iberdrola") || cp.contains("Movistar") || desc_lower.contains("electricity") || desc_lower.contains("internet") {
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
    let mut balance_at: std::collections::HashMap<NaiveDate, f64> = std::collections::HashMap::new();
    let mut bal = final_balance;
    // Walk from today backwards; at each snapshot date, record the balance,
    // then subtract any transactions on that date (to get the balance before them).
    for &sd in snapshot_dates.iter().rev() {
        balance_at.insert(sd, bal);
        let day_total: f64 = sorted.iter()
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
            pool, account_id, snap_balance, "ITAV", &timestamp,
        ).await?;

        // Insert transactions for this date, linked to this snapshot
        for (txd, i, amount, _, currency, desc, cp) in &sorted {
            if *txd == sd {
                let eid = format!("{prefix}_{i:03}");
                let result = sqlx::query(
                    "INSERT OR IGNORE INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, counterparty, snapshot_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
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
    sqlx::query("UPDATE leanfin_accounts SET balance_amount = ?, balance_currency = 'EUR' WHERE id = ?")
        .bind(final_balance)
        .bind(account_id)
        .execute(pool)
        .await?;

    Ok(count)
}

/// Seed sparse balance snapshots for a manual account.
async fn seed_manual_balances(pool: &SqlitePool, account_id: i64, values: &[(&str, f64)]) -> Result<()> {
    let mut last_value = 0.0;
    for (date, value) in values {
        let timestamp = format!("{date}T23:59:59Z");
        sqlx::query(
            r#"INSERT OR IGNORE INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type)
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
