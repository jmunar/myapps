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

    // Delete all daily balances
    sqlx::query("DELETE FROM daily_balances")
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
