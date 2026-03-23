#[tokio::test]
async fn accounts_page_shows_both_sections() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("Bank Accounts"),
        "missing Bank Accounts heading"
    );
    assert!(
        body.contains("Manual Accounts"),
        "missing Manual Accounts heading"
    );
}

#[tokio::test]
async fn accounts_page_shows_manual_account_from_seed() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(
        body.contains("Stock Portfolio"),
        "missing Stock Portfolio name"
    );
    assert!(body.contains("investment"), "missing investment category");
}

#[tokio::test]
async fn manual_account_has_action_buttons() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Update value"), "missing Update value button");
    assert!(body.contains("Edit"), "missing Edit button");
    assert!(body.contains("Delete"), "missing Delete button");
}

#[tokio::test]
async fn new_manual_account_form_renders() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts/manual/new").await;
    let body = response.text();
    assert!(body.contains("Add Manual Account"), "missing form title");
    assert!(body.contains(r#"name="name""#), "missing name field");
    assert!(
        body.contains(r#"name="category""#),
        "missing category field"
    );
    assert!(
        body.contains(r#"name="currency""#),
        "missing currency field"
    );
    assert!(
        body.contains(r#"name="initial_value""#),
        "missing initial_value field"
    );
    assert!(body.contains(r#"name="date""#), "missing date field");
}

#[tokio::test]
async fn create_manual_account() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let response = app
        .server
        .post("/leanfin/accounts/manual/new")
        .form(&serde_json::json!({
            "name": "My Crypto Wallet",
            "category": "crypto",
            "currency": "EUR",
            "initial_value": "5000.00",
            "date": "2026-03-01",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    // Verify it appears on the list page
    let list = app.server.get("/leanfin/accounts").await;
    let body = list.text();
    assert!(
        body.contains("My Crypto Wallet"),
        "new account not on list page"
    );
    assert!(body.contains("crypto"), "category not on list page");
}

#[tokio::test]
async fn edit_manual_account_form_renders() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/edit"))
        .await;
    let body = response.text();
    assert!(body.contains("Edit Account"), "missing edit form title");
    assert!(
        body.contains("Stock Portfolio"),
        "missing prefilled account name"
    );
    assert!(body.contains(r#"name="name""#), "missing name field");
    assert!(
        body.contains(r#"name="category""#),
        "missing category field"
    );
}

#[tokio::test]
async fn edit_manual_account_updates_name() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/edit"))
        .form(&serde_json::json!({
            "name": "Bond Portfolio",
            "category": "investment",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/leanfin/accounts").await;
    let body = list.text();
    assert!(
        body.contains("Bond Portfolio"),
        "updated name not found on list page"
    );
    assert!(
        !body.contains("Stock Portfolio"),
        "old name still on list page"
    );
}

#[tokio::test]
async fn value_update_form_renders() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/value"))
        .await;
    let body = response.text();
    assert!(body.contains("Update Value"), "missing value form title");
    assert!(
        body.contains("Stock Portfolio"),
        "missing account name in form"
    );
    assert!(body.contains(r#"name="value""#), "missing value field");
    assert!(body.contains(r#"name="date""#), "missing date field");
}

#[tokio::test]
async fn value_update_records_balance_snapshot() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/value"))
        .form(&serde_json::json!({
            "value": "17500.00",
            "date": "2026-03-13",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    // Verify balance_amount updated in DB
    let (balance,): (f64,) =
        sqlx::query_as("SELECT balance_amount FROM leanfin_accounts WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(
        (balance - 17500.0).abs() < 0.01,
        "balance_amount not updated: {balance}"
    );

    // Verify balance_snapshots entry created
    let (db_balance,): (f64,) = sqlx::query_as(
        "SELECT balance FROM leanfin_balance_snapshots WHERE account_id = ? AND date = '2026-03-13' AND balance_type = 'MANUAL'",
    )
    .bind(id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        (db_balance - 17500.0).abs() < 0.01,
        "balance snapshot not recorded: {db_balance}"
    );
}

#[tokio::test]
async fn delete_manual_account() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/leanfin/accounts").await;
    let body = list.text();
    assert!(
        !body.contains("Stock Portfolio"),
        "deleted account still on list page"
    );
}

#[tokio::test]
async fn balance_evolution_dropdown_shows_manual_account_name() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        body.contains("Stock Portfolio"),
        "manual account not in balance evolution dropdown"
    );
    // Should not show "null" in the display
    assert!(
        !body.contains("(null)"),
        "dropdown shows (null) for manual account"
    );
}

#[tokio::test]
async fn sync_skips_manual_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Sync should only attempt bank accounts; manual accounts should not cause errors
    // With seeded data we have 2 bank accounts (which will fail due to fake sessions)
    // and 1 manual account which should be silently skipped.
    let response = app.server.post("/leanfin/sync").await;
    let body = response.text();
    // The sync reports errors for the 2 bank accounts but does NOT mention the manual account
    assert!(
        body.contains("sync-status"),
        "sync response missing status indicator"
    );
    // Verify manual account still exists and is untouched
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM leanfin_accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "manual account should still exist after sync");
}
