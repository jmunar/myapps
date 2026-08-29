#[tokio::test]
async fn balance_evolution_page_renders_with_nav_and_controls() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Balance Evolution"));
    assert!(body.contains("balance-controls"));
    assert!(body.contains("period-selector"));
}

#[tokio::test]
async fn balance_evolution_page_shows_balance_nav_active() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    // The "Balance" nav item should be marked active
    assert!(body.contains(r#"active"#));
    assert!(body.contains("/leanfin/balance-evolution"));
    assert!(body.contains("Balance"));
}

#[tokio::test]
async fn balance_evolution_page_has_all_accounts_option() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains(r#"<option value="">All accounts</option>"#));
}

#[tokio::test]
async fn balance_evolution_page_has_individual_account_options() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Santander"));
    assert!(body.contains("ING Direct"));
}

#[tokio::test]
async fn balance_evolution_page_has_period_buttons() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains(">30d</button>"));
    assert!(body.contains(">90d</button>"));
    assert!(body.contains(">180d</button>"));
    assert!(body.contains(">365d</button>"));
}

#[tokio::test]
async fn data_endpoint_returns_script_calling_update_balance_chart() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // Data endpoint returns a script tag calling updateBalanceChart with JSON arrays
    assert!(body.contains("updateBalanceChart("));
    assert!(body.contains("<script>"));
}

#[tokio::test]
async fn data_endpoint_returns_script_when_account_id_empty() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // Aggregated balance also returns updateBalanceChart script
    assert!(body.contains("updateBalanceChart("));
}

#[tokio::test]
async fn data_endpoint_returns_empty_state_when_no_balance_data() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Delete all balance snapshots
    sqlx::query("DELETE FROM leanfin_balance_snapshots")
        .execute(&app.pool)
        .await
        .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // Empty state is now shown via showBalanceEmpty script call
    assert!(body.contains("showBalanceEmpty("));
    assert!(body.contains("No balance data yet"));
}

#[tokio::test]
async fn data_endpoint_returns_not_found_for_other_users_account() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Use an account ID that doesn't belong to the user
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "99999")
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // Not-found is shown via showBalanceEmpty script call
    assert!(body.contains("showBalanceEmpty("));
    assert!(body.contains("Account not found"));
}

#[tokio::test]
async fn data_endpoint_contains_balance_data_as_json_arrays() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // Data is passed as JSON arrays to updateBalanceChart(dates, values, accountId)
    assert!(body.contains("updateBalanceChart(["));
    // Contains date strings in the first array
    assert!(body.contains("\"20"));
}

#[tokio::test]
async fn data_endpoint_passes_account_id_to_chart_function() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // The account ID is passed as the third argument to updateBalanceChart
    assert!(body.contains(&format!("'{account_id}'")));
}

#[tokio::test]
async fn balance_page_chart_config_uses_accent_color() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // The accent color is now in the page template (Chart.js config), not the data endpoint
    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("#1A6B5A"));
}

#[tokio::test]
async fn single_snapshot_with_historical_transactions_shows_full_series() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    // Create an account with one snapshot (today) and transactions spanning multiple days
    let user_id: (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type) VALUES (?, 'TestBank', 'ES', 'sess', 'uid_test', '2027-01-01T00:00:00Z', 'bank')"
    )
    .bind(user_id.0)
    .execute(&app.pool)
    .await
    .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'TestBank'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Insert a single snapshot at today with balance 1000
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let timestamp = format!("{today}T06:00:00Z");
    let snap_result = sqlx::query(
        "INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type) VALUES (?, ?, ?, 1000.0, 'ITAV')"
    )
    .bind(account_id)
    .bind(&timestamp)
    .bind(&today)
    .execute(&app.pool)
    .await
    .unwrap();
    let snap_id = snap_result.last_insert_rowid();

    // Insert transactions on earlier dates, linked to this snapshot
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let two_days_ago = (chrono::Utc::now() - chrono::Duration::days(2))
        .format("%Y-%m-%d")
        .to_string();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx1', ?, -50.0, 'EUR', 'Purchase', ?)"
    )
    .bind(account_id)
    .bind(&yesterday)
    .bind(snap_id)
    .execute(&app.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx2', ?, -100.0, 'EUR', 'Big purchase', ?)"
    )
    .bind(account_id)
    .bind(&two_days_ago)
    .bind(snap_id)
    .execute(&app.pool)
    .await
    .unwrap();

    // Fetch balance data — should show multiple days, not just today
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "30")
        .await;
    let body = response.text();

    // Should contain the chart update call (not empty state)
    assert!(body.contains("updateBalanceChart("), "should render chart");

    // Should contain dates from at least 2 days ago (backward walk worked)
    assert!(
        body.contains(&two_days_ago),
        "chart should include date from 2 days ago: {two_days_ago}\nbody: {body}"
    );
    assert!(
        body.contains(&yesterday),
        "chart should include yesterday: {yesterday}"
    );
    assert!(body.contains(&today), "chart should include today");
}

#[tokio::test]
async fn balance_page_chart_has_click_drill_down() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // The onClick handler and loadBalanceTxn are now in the page template
    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        body.contains("onClick"),
        "chart config should have onClick handler"
    );
    assert!(
        body.contains("loadBalanceTxn"),
        "chart should call loadBalanceTxn on click"
    );
}

#[tokio::test]
async fn balance_evolution_page_has_transaction_drill_down_card() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        body.contains("balance-txn-card"),
        "page should have hidden transaction card"
    );
    assert!(
        body.contains("balance-txn-table"),
        "page should have transaction table container"
    );
    assert!(
        body.contains("loadBalanceTxn"),
        "page should define loadBalanceTxn function"
    );
}

#[tokio::test]
async fn balance_evolution_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    // Do NOT log in
    let response = app
        .server
        .get("/leanfin/balance-evolution")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn data_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    // Do NOT log in
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn balance_evolution_page_has_persistent_canvas_in_chart_container() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();

    // Canvas is persistent in the page template
    assert!(body.contains("<canvas"));
    assert!(body.contains(r#"id="balance-canvas""#));
    assert!(body.contains("chart-container"));
    // The updateBalanceChart function is defined in the page
    assert!(body.contains("updateBalanceChart"));
    // The showBalanceEmpty function is defined in the page
    assert!(body.contains("showBalanceEmpty"));
}

#[tokio::test]
async fn balance_evolution_page_chart_config_uses_line_type() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();

    // Chart.js line chart configuration in the page template
    assert!(body.contains("type: 'line'"));
    assert!(body.contains("fill: true"));
}

#[tokio::test]
async fn data_endpoint_returns_empty_account_id_for_aggregated() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    // When aggregated (no account_id), the third param should be empty string
    assert!(body.contains("''"));
}

/// Insert a manual account with a single balance entry `days_ago` days in the past.
async fn insert_stale_manual_account(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    name: &str,
    balance: f64,
    days_ago: i64,
    archived: bool,
) -> i64 {
    let uid = format!("manual_{name}");
    let account_id = sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type, account_name, balance_amount, balance_currency, archived) VALUES (?, ?, '', '', ?, '9999-12-31T00:00:00Z', 'manual', ?, ?, 'EUR', ?)",
    )
    .bind(user_id)
    .bind(name)
    .bind(&uid)
    .bind(name)
    .bind(balance)
    .bind(archived)
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let date = (chrono::Utc::now() - chrono::Duration::days(days_ago))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query(
        "INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type) VALUES (?, ?, ?, ?, 'MANUAL')",
    )
    .bind(account_id)
    .bind(format!("{date}T23:59:59Z"))
    .bind(&date)
    .bind(balance)
    .execute(pool)
    .await
    .unwrap();

    account_id
}

#[tokio::test]
async fn manual_account_without_recent_entries_still_shows_balance() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Last updated 200 days ago — well outside the 90-day window
    let account_id =
        insert_stale_manual_account(&app.pool, user_id, "Old Portfolio", 5000.0, 200, false).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("days", "90")
        .await;
    let body = response.text();

    assert!(
        body.contains("updateBalanceChart("),
        "stale manual account should still render a series, got: {body}"
    );
    assert!(
        body.contains("5000.00"),
        "series should carry the last known balance forward, got: {body}"
    );
}

#[tokio::test]
async fn aggregated_series_includes_manual_account_without_recent_entries() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // One account updated inside the window, one untouched for 200 days
    insert_stale_manual_account(&app.pool, user_id, "Current Account", 1000.0, 10, false).await;
    insert_stale_manual_account(&app.pool, user_id, "Old Portfolio", 5000.0, 200, false).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();

    assert!(
        body.contains("6000.00"),
        "total should include the account with no recent entries, got: {body}"
    );
}

#[tokio::test]
async fn aggregated_series_excludes_archived_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    insert_stale_manual_account(&app.pool, user_id, "Current Account", 1000.0, 10, false).await;
    insert_stale_manual_account(&app.pool, user_id, "Closed Account", 7000.0, 200, true).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();

    assert!(
        body.contains("1000.00"),
        "total should include the active account, got: {body}"
    );
    assert!(
        !body.contains("8000.00"),
        "archived account should not contribute to the total, got: {body}"
    );
}
