use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use super::super::settings::{self, EnableBankingCredentials};
use super::enable_banking;
use crate::config::Config;
use crate::models::Account;

/// Result of a sync operation, used to report status in the UI.
pub struct SyncResult {
    pub total_new: u64,
    pub accounts_synced: u64,
    pub accounts_skipped: u64,
    pub errors: Vec<String>,
    pub reconciliation_warnings: Vec<String>,
}

pub async fn run(pool: &SqlitePool, config: &Config) -> Result<()> {
    tracing::info!("Starting transaction sync");

    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT id, user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, balance_amount, balance_currency, account_type, account_name, asset_category, archived, created_at FROM leanfin_accounts WHERE account_type = 'bank' AND archived = 0",
    )
    .fetch_all(pool)
    .await?;

    if accounts.is_empty() {
        tracing::info!("No linked bank accounts, nothing to sync");
        return Ok(());
    }

    // Group accounts by user_id to fetch credentials once per user
    let mut by_user: std::collections::HashMap<i64, Vec<&Account>> =
        std::collections::HashMap::new();
    for account in &accounts {
        by_user.entry(account.user_id).or_default().push(account);
    }

    let mut total_new = 0u64;
    let now = Utc::now().naive_utc();

    for (user_id, user_accounts) in &by_user {
        let creds = match settings::get_credentials(pool, config, *user_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "User {user_id}: skipping sync — Enable Banking credentials not available: {e:#}"
                );
                continue;
            }
        };

        for account in user_accounts {
            let days_until_expiry = (account.session_expires_at - now).num_days();

            if days_until_expiry <= 0 {
                tracing::warn!(
                    "Account '{}' ({}): session expired, skipping",
                    account.bank_name,
                    account.id
                );
                crate::services::notify::send(
                    pool,
                    config,
                    "LeanFin",
                    &format!(
                        "Bank session expired for '{}'. Please re-authorize.",
                        account.bank_name
                    ),
                )
                .await;
                continue;
            }

            if days_until_expiry <= 7 {
                crate::services::notify::send(
                    pool,
                    config,
                    "LeanFin",
                    &format!(
                        "Bank session for '{}' expires in {} days. Re-authorize soon.",
                        account.bank_name, days_until_expiry
                    ),
                )
                .await;
            }

            match sync_account(pool, config, &creds, account).await {
                Ok((count, _warning)) => {
                    tracing::info!(
                        "Account '{}': {} new transactions",
                        account.bank_name,
                        count
                    );
                    total_new += count;
                }
                Err(e) => {
                    tracing::error!("Account '{}': sync failed: {e:#}", account.bank_name);
                }
            }
        }
    }

    tracing::info!(
        "Sync complete: {total_new} new transactions across {} accounts",
        accounts.len()
    );

    Ok(())
}

/// Returns (inserted_count, optional reconciliation warning).
async fn sync_account(
    pool: &SqlitePool,
    config: &Config,
    creds: &EnableBankingCredentials,
    account: &Account,
) -> Result<(u64, Option<String>)> {
    let account_uid = &account.account_uid;

    // 1. Fetch and store balance snapshot first (non-fatal)
    let mut snapshot_id: Option<i64> = None;
    let mut best_balance: Option<(f64, String)> = None; // (amount, balance_type)
    match enable_banking::get_balances(pool, creds, account_uid, Some(account.id)).await {
        Ok(balances) => {
            if let Some(best) = enable_banking::pick_best_balance(&balances)
                && let Ok(amount) = best.balance_amount.amount.parse::<f64>()
            {
                let currency = &best.balance_amount.currency;
                best_balance = Some((amount, best.balance_type.clone()));

                if let Err(e) = sqlx::query(
                        "UPDATE leanfin_accounts SET balance_amount = ?, balance_currency = ? WHERE id = ?",
                    )
                    .bind(amount)
                    .bind(currency)
                    .bind(account.id)
                    .execute(pool)
                    .await
                    {
                        tracing::warn!("Failed to update balance for '{}': {e:#}", account.bank_name);
                    }

                let timestamp = super::balance::timestamp_for_balance_type(
                    &best.balance_type,
                    best.reference_date.as_deref(),
                );
                match super::balance::record_balance_snapshot(
                    pool,
                    account.id,
                    amount,
                    &best.balance_type,
                    &timestamp,
                )
                .await
                {
                    Ok(id) => snapshot_id = Some(id),
                    Err(e) => tracing::warn!(
                        "Failed to record balance snapshot for '{}': {e:#}",
                        account.bank_name
                    ),
                }
            }
        }
        Err(e) => {
            anyhow::bail!(
                "Failed to fetch balances for '{}': {e:#}",
                account.bank_name
            );
        }
    }

    // 2. Fetch and insert transactions, linking to the snapshot
    let has_transactions: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM leanfin_transactions WHERE account_id = ?)",
    )
    .bind(account.id)
    .fetch_one(pool)
    .await?;

    let lookback_days = if has_transactions { 5 } else { 90 };
    let date_from = (Utc::now() - Duration::days(lookback_days))
        .format("%Y-%m-%d")
        .to_string();

    let transactions =
        enable_banking::get_transactions(pool, creds, account_uid, &date_from, Some(account.id))
            .await?;

    let mut inserted = 0u64;

    for tx in &transactions {
        let external_id = tx.external_id();
        let date = tx.date();
        let raw_amount: f64 = tx.transaction_amount.amount.parse().unwrap_or(0.0);
        let amount = match tx.credit_debit_indicator.as_deref() {
            Some("DBIT") => -raw_amount.abs(),
            Some("CRDT") => raw_amount.abs(),
            _ => raw_amount,
        };
        let currency = &tx.transaction_amount.currency;
        let description = tx.description();
        let counterparty = tx.counterparty();
        let balance_after: Option<f64> = tx
            .balance_after_transaction
            .as_ref()
            .and_then(|b| b.amount.as_deref())
            .and_then(|a| a.parse().ok());

        let result = sqlx::query(
            r#"INSERT OR IGNORE INTO leanfin_transactions
               (account_id, external_id, date, amount, currency, description, counterparty, balance_after, snapshot_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(account.id)
        .bind(&external_id)
        .bind(&date)
        .bind(amount)
        .bind(currency)
        .bind(&description)
        .bind(&counterparty)
        .bind(balance_after)
        .bind(snapshot_id)
        .execute(pool)
        .await?;

        inserted += result.rows_affected();
    }

    // 3. Reconciliation (ITAV only, using snapshot-linked transactions)
    let mut reconciliation_warning: Option<String> = None;
    if let Some((balance, ref balance_type)) = best_balance {
        match super::balance::check_reconciliation(
            pool,
            config,
            account,
            balance,
            balance_type,
            snapshot_id,
        )
        .await
        {
            Ok(warning) => reconciliation_warning = warning,
            Err(e) => tracing::warn!(
                "Reconciliation check failed for '{}': {e:#}",
                account.bank_name
            ),
        }
    }

    // 4. Auto-labeling on newly fetched transactions
    if inserted > 0 {
        match super::labeling::apply_rules(pool, account.user_id).await {
            Ok(labeled) => {
                if labeled > 0 {
                    tracing::info!(
                        "Account '{}': auto-labeled {labeled} transactions",
                        account.bank_name
                    );
                }
            }
            Err(e) => tracing::warn!("Auto-labeling failed: {e:#}"),
        }
    }

    Ok((inserted, reconciliation_warning))
}

/// Run sync for a single user's accounts, returning a structured result for UI display.
pub async fn run_for_user(pool: &SqlitePool, config: &Config, user_id: i64) -> SyncResult {
    tracing::info!("Starting sync for user {user_id}");

    let mut result = SyncResult {
        total_new: 0,
        accounts_synced: 0,
        accounts_skipped: 0,
        errors: Vec::new(),
        reconciliation_warnings: Vec::new(),
    };

    let creds = match settings::get_credentials(pool, config, user_id).await {
        Ok(c) => c,
        Err(_) => {
            tracing::info!(
                "User {user_id}: no Enable Banking credentials configured, skipping sync"
            );
            return result;
        }
    };

    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT id, user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, balance_amount, balance_currency, account_type, account_name, asset_category, archived, created_at FROM leanfin_accounts WHERE user_id = ? AND account_type = 'bank' AND archived = 0",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let now = Utc::now().naive_utc();

    for account in &accounts {
        let days_until_expiry = (account.session_expires_at - now).num_days();

        if days_until_expiry <= 0 {
            result.accounts_skipped += 1;
            result
                .errors
                .push(format!("'{}': session expired", account.bank_name,));
            continue;
        }

        match sync_account(pool, config, &creds, account).await {
            Ok((count, warning)) => {
                result.total_new += count;
                result.accounts_synced += 1;
                if let Some(w) = warning {
                    result.reconciliation_warnings.push(w);
                }
            }
            Err(e) => {
                result.accounts_skipped += 1;
                result
                    .errors
                    .push(format!("'{}': {e:#}", account.bank_name,));
            }
        }
    }

    tracing::info!(
        "User {user_id} sync complete: {} new transactions, {} accounts synced, {} errors",
        result.total_new,
        result.accounts_synced,
        result.errors.len(),
    );

    result
}
