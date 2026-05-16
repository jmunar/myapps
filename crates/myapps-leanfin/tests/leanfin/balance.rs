use myapps_leanfin::services::balance::record_balance_snapshot;

// Regression: recording a second snapshot for the same (account, balance_type,
// date) must keep the row id stable, so that transactions previously linked to
// it via snapshot_id are not orphaned. The original implementation did
// DELETE-then-INSERT, which triggered the snapshot_id ON DELETE SET NULL
// cascade and produced the balance discrepancy we hit in production.
#[tokio::test]
async fn same_day_resync_preserves_transaction_snapshot_link() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type)
         VALUES (?, 'TestBank', 'ES', 'sess', 'uid_snapshot_regression', '2027-01-01T00:00:00Z', 'bank')"
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let (account_id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_uid = 'uid_snapshot_regression'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let timestamp_1 = "2026-05-15T06:00:00Z";
    let first_id = record_balance_snapshot(&app.pool, account_id, 1000.0, "ITAV", timestamp_1)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, snapshot_id)
         VALUES (?, 'tx_regression', '2026-05-15', -50.0, 'EUR', 'Coffee', ?)"
    )
    .bind(account_id)
    .bind(first_id)
    .execute(&app.pool)
    .await
    .unwrap();

    // Simulate a same-day re-sync: different timestamp + balance, same date.
    let timestamp_2 = "2026-05-15T14:30:00Z";
    let second_id = record_balance_snapshot(&app.pool, account_id, 950.0, "ITAV", timestamp_2)
        .await
        .unwrap();

    assert_eq!(
        first_id, second_id,
        "upsert must keep the snapshot row id stable across same-day re-syncs"
    );

    let (snap_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM leanfin_balance_snapshots WHERE account_id = ? AND balance_type = 'ITAV' AND date = '2026-05-15'",
    )
    .bind(account_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(snap_count, 1, "only one snapshot row per account/type/date");

    let (linked_snapshot_id,): (Option<i64>,) = sqlx::query_as(
        "SELECT snapshot_id FROM leanfin_transactions WHERE external_id = 'tx_regression' AND account_id = ?",
    )
    .bind(account_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        linked_snapshot_id,
        Some(first_id),
        "transaction snapshot_id must still point at the snapshot (not NULL) after re-sync"
    );

    let (stored_balance, stored_timestamp): (f64, String) =
        sqlx::query_as("SELECT balance, timestamp FROM leanfin_balance_snapshots WHERE id = ?")
            .bind(first_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(
        (stored_balance - 950.0).abs() < 0.001,
        "balance should be updated to the latest value"
    );
    assert_eq!(
        stored_timestamp, timestamp_2,
        "timestamp should be updated to the latest value"
    );
}
