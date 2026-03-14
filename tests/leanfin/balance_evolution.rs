use crate::harness;

#[tokio::test]
async fn balance_evolution_page_renders_with_nav_and_controls() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Balance Evolution"));
    assert!(body.contains("balance-controls"));
    assert!(body.contains("period-selector"));
}

#[tokio::test]
async fn balance_evolution_page_shows_balance_nav_active() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    // The "Balance" nav item should be marked active
    assert!(body.contains(r#"active"#));
    assert!(body.contains("/leanfin/balance-evolution"));
    assert!(body.contains("Balance"));
}

#[tokio::test]
async fn balance_evolution_page_has_all_accounts_option() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains(r#"<option value="">All accounts</option>"#));
}

#[tokio::test]
async fn balance_evolution_page_has_individual_account_options() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Santander"));
    assert!(body.contains("ING Direct"));
}

#[tokio::test]
async fn balance_evolution_page_has_period_buttons() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains(">30d</button>"));
    assert!(body.contains(">90d</button>"));
    assert!(body.contains(">180d</button>"));
    assert!(body.contains(">365d</button>"));
}

#[tokio::test]
async fn data_endpoint_returns_frappe_chart_for_specific_account() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    assert!(body.contains("balance-chart"));
    assert!(body.contains("frappe-chart-container"));
    assert!(body.contains("frappe.Chart"));
}

#[tokio::test]
async fn data_endpoint_returns_chart_when_account_id_empty() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    assert!(body.contains("frappe-chart-container"));
    assert!(body.contains("frappe.Chart"));
}

#[tokio::test]
async fn data_endpoint_returns_empty_state_when_no_balance_data() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Delete all balance snapshots
    sqlx::query("DELETE FROM balance_snapshots")
        .execute(&app.pool)
        .await
        .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    assert!(body.contains("No balance data yet"));
    assert!(body.contains("empty-state"));
}

#[tokio::test]
async fn data_endpoint_returns_not_found_for_other_users_account() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Use an account ID that doesn't belong to the user
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "99999")
        .add_query_param("days", "90")
        .await;
    let body = response.text();
    assert!(body.contains("Account not found"));
}

#[tokio::test]
async fn data_endpoint_contains_balance_data_in_json() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    // Chart data is embedded as JSON arrays in the script
    assert!(body.contains("labels:"));
    assert!(body.contains("values:"));
    assert!(body.contains("type: 'line'"));
}

#[tokio::test]
async fn data_endpoint_renders_frappe_chart_container() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    assert!(body.contains("balance-chart"));
    assert!(body.contains("frappe-chart-container"));
    assert!(body.contains("regionFill"));
}

#[tokio::test]
async fn data_endpoint_uses_accent_color() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    // Chart uses the app's accent color
    assert!(body.contains("#1A6B5A"));
}

#[tokio::test]
async fn single_snapshot_with_historical_transactions_shows_full_series() {
    let app = harness::spawn_app().await;
    app.login_as("demo", "demo").await;

    // Create an account with one snapshot (today) and transactions spanning multiple days
    let user_id: (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type) VALUES (?, 'TestBank', 'ES', 'sess', 'uid_test', '2027-01-01T00:00:00Z', 'bank')"
    )
    .bind(user_id.0)
    .execute(&app.pool)
    .await
    .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'TestBank'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Insert a single snapshot at today with balance 1000
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let timestamp = format!("{today}T06:00:00Z");
    let snap_result = sqlx::query(
        "INSERT INTO balance_snapshots (account_id, timestamp, date, balance, balance_type) VALUES (?, ?, ?, 1000.0, 'ITAV')"
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
        "INSERT INTO transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx1', ?, -50.0, 'EUR', 'Purchase', ?)"
    )
    .bind(account_id)
    .bind(&yesterday)
    .bind(snap_id)
    .execute(&app.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx2', ?, -100.0, 'EUR', 'Big purchase', ?)"
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

    // Should contain the chart (not empty state)
    assert!(body.contains("frappe.Chart"), "should render chart");

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
async fn data_endpoint_chart_is_navigable_with_drill_down() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'Santander'")
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
    assert!(body.contains("isNavigable: true"), "chart should be navigable");
    assert!(
        body.contains("data-select"),
        "chart should have data-select event listener"
    );
    assert!(
        body.contains("loadBalanceTxn"),
        "chart should call loadBalanceTxn on click"
    );
}

#[tokio::test]
async fn balance_evolution_page_has_transaction_drill_down_card() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
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
    let app = harness::spawn_app().await;
    // Do NOT log in
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}
