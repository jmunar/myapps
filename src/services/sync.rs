use anyhow::Result;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::models::Account;

pub async fn run(pool: &SqlitePool, config: &Config) -> Result<()> {
    tracing::info!("Starting transaction sync");

    let accounts: Vec<Account> = sqlx::query_as(
        "SELECT id, user_id, bank_name, iban, enable_banking_id, access_token_enc, token_expires_at, created_at FROM accounts",
    )
    .fetch_all(pool)
    .await?;

    if accounts.is_empty() {
        tracing::info!("No linked accounts, nothing to sync");
        return Ok(());
    }

    let mut total_new = 0u64;
    let now = chrono::Utc::now().naive_utc();

    for account in &accounts {
        // Check token expiry
        let days_until_expiry = (account.token_expires_at - now).num_days();

        if days_until_expiry <= 0 {
            tracing::warn!(
                "Account '{}' ({}): token expired, skipping",
                account.bank_name,
                account.id
            );
            super::notify::send(
                config,
                &format!(
                    "Bank token expired for account '{}'. Please re-authorize.",
                    account.bank_name
                ),
            )
            .await;
            continue;
        }

        if days_until_expiry <= 7 {
            super::notify::send(
                config,
                &format!(
                    "Bank token for '{}' expires in {} days. Please re-authorize soon.",
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

async fn sync_account(
    _pool: &SqlitePool,
    config: &Config,
    account: &Account,
) -> Result<u64> {
    let _access_token =
        crate::services::enable_banking::decrypt_token(&account.access_token_enc, config)?;

    // TODO: Fetch transactions from Enable Banking API
    // TODO: Insert with ON CONFLICT DO NOTHING
    // TODO: Run auto-labeling on new transactions

    Ok(0)
}
