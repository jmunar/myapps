use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use crate::config::Config;
use crate::models::Account;
use super::enable_banking;

pub async fn run(pool: &SqlitePool, config: &Config) -> Result<()> {
    tracing::info!("Starting transaction sync");

    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT id, user_id, bank_name, bank_country, iban, session_id, account_uid, session_expires_at, created_at FROM accounts",
    )
    .fetch_all(pool)
    .await?;

    if accounts.is_empty() {
        tracing::info!("No linked accounts, nothing to sync");
        return Ok(());
    }

    let mut total_new = 0u64;
    let now = Utc::now().naive_utc();

    for account in &accounts {
        let days_until_expiry = (account.session_expires_at - now).num_days();

        if days_until_expiry <= 0 {
            tracing::warn!(
                "Account '{}' ({}): session expired, skipping",
                account.bank_name,
                account.id
            );
            crate::services::notify::send(
                config,
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
                config,
                &format!(
                    "Bank session for '{}' expires in {} days. Re-authorize soon.",
                    account.bank_name, days_until_expiry
                ),
            )
            .await;
        }

        match sync_account(pool, config, account).await {
            Ok(count) => {
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

    tracing::info!(
        "Sync complete: {total_new} new transactions across {} accounts",
        accounts.len()
    );

    Ok(())
}

async fn sync_account(pool: &SqlitePool, config: &Config, account: &Account) -> Result<u64> {
    // Fetch last 5 days for overlap safety
    let date_from = (Utc::now() - Duration::days(5)).format("%Y-%m-%d").to_string();

    let transactions =
        enable_banking::get_transactions(config, &account.account_uid, &date_from).await?;

    let mut inserted = 0u64;

    for tx in &transactions {
        let external_id = tx.external_id();
        let date = tx.date();
        let amount: f64 = tx.transaction_amount.amount.parse().unwrap_or(0.0);
        let currency = &tx.transaction_amount.currency;
        let description = tx.description();
        let counterparty = tx.counterparty();
        let balance_after: Option<f64> = tx
            .balance_after_transaction
            .as_ref()
            .and_then(|b| b.amount.as_deref())
            .and_then(|a| a.parse().ok());

        let result = sqlx::query(
            r#"INSERT OR IGNORE INTO transactions
               (account_id, external_id, date, amount, currency, description, counterparty, balance_after)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(account.id)
        .bind(&external_id)
        .bind(&date)
        .bind(amount)
        .bind(currency)
        .bind(&description)
        .bind(&counterparty)
        .bind(balance_after)
        .execute(pool)
        .await?;

        inserted += result.rows_affected();
    }

    // Run auto-labeling on newly fetched transactions
    if inserted > 0 {
        match super::labeling::apply_rules(pool, account.user_id).await {
            Ok(labeled) => {
                if labeled > 0 {
                    tracing::info!("Account '{}': auto-labeled {labeled} transactions", account.bank_name);
                }
            }
            Err(e) => tracing::warn!("Auto-labeling failed: {e:#}"),
        }
    }

    Ok(inserted)
}
