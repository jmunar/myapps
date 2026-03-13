use crate::harness;

// ── Dashboard label filter ───────────────────────────────────

#[tokio::test]
async fn dashboard_has_label_ids_filter() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    // label_ids select filter exists in the dashboard
    assert!(body.contains(r#"name="label_ids""#));
    assert!(body.contains("All labels"));
    assert!(body.contains("Groceries"));
}

#[tokio::test]
async fn dashboard_nav_includes_expenses_tab() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    assert!(body.contains("/leanfin/expenses"));
    assert!(body.contains("Expenses"));
}

// ── Transaction label_ids filter ─────────────────────────────

#[tokio::test]
async fn transaction_label_ids_filter_returns_matching() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("label_ids", &label_id.to_string())
        .await;
    let body = response.text();

    // Groceries-labeled transactions include Mercadona
    assert!(body.contains("Mercadona"));
    // Should not contain transactions only labeled differently
    assert!(!body.contains("Netflix"));
}

#[tokio::test]
async fn transaction_label_ids_filter_empty_returns_all() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Empty label_ids param should not filter (returns all transactions)
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("label_ids", "")
        .await;
    let body = response.text();

    assert!(body.contains("Mercadona"));
    assert!(body.contains("Netflix"));
}

// ── Transaction date_from / date_to filters ──────────────────

#[tokio::test]
async fn transaction_date_from_filter() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Use a far-future date so nothing matches
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_from", "2099-01-01")
        .await;
    let body = response.text();

    // No transactions in the future → empty state
    assert!(body.contains("No transactions yet"));
}

#[tokio::test]
async fn transaction_date_to_filter() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Use a far-past date so nothing matches
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_to", "1900-01-01")
        .await;
    let body = response.text();

    assert!(body.contains("No transactions yet"));
}

#[tokio::test]
async fn transaction_date_range_filter_returns_subset() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Get a date that exists in seed data
    let (some_date,): (String,) =
        sqlx::query_as("SELECT date FROM transactions ORDER BY date DESC LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_from", &some_date)
        .add_query_param("date_to", &some_date)
        .await;
    let body = response.text();

    // Should return at least the transaction(s) on that date
    assert!(response.status_code().is_success());
    assert!(body.contains(&some_date));
}

#[tokio::test]
async fn dashboard_loads_with_htmx_container() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();
    assert!(body.contains("Transactions"));
    assert!(body.contains("hx-get"));
}

#[tokio::test]
async fn transaction_list_returns_table() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("Mercadona"));
    assert!(body.contains("Netflix"));
}

#[tokio::test]
async fn transaction_search_filters_results() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("q", "Netflix")
        .await;
    let body = response.text();
    assert!(body.contains("Netflix"));
    assert!(!body.contains("Mercadona"));
}

#[tokio::test]
async fn transaction_unallocated_filter() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("unallocated", "1")
        .await;
    let body = response.text();
    // The seed data leaves some transactions unallocated (the else-continue branch)
    // We just verify it returns a valid response
    assert!(response.status_code().is_success());
    assert!(!body.is_empty());
}

#[tokio::test]
async fn transaction_account_filter() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Get a specific account id from the DB
    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM accounts WHERE bank_name = 'ING Direct'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("account_id", &account_id.to_string())
        .await;
    let body = response.text();
    // ING Direct savings account has "MyInvestor" and "Self transfer" transactions
    assert!(body.contains("Self transfer") || body.contains("MyInvestor"));
    // Should not contain checking-only transactions
    assert!(!body.contains("Mercadona"));
}

#[tokio::test]
async fn transaction_list_has_balance_column() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("<th>Balance</th>"));
}

#[tokio::test]
async fn transaction_balance_shows_value_when_present() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Pick a transaction and set its balance_after
    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM transactions LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    sqlx::query("UPDATE transactions SET balance_after = ? WHERE id = ?")
        .bind(1500.50_f64)
        .bind(txn_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("1500.50"));
    assert!(body.contains(r#"class="txn-balance""#));
}

#[tokio::test]
async fn transaction_balance_shows_dash_when_null() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    // Seed data has no balance_after set, so all balances should show "—"
    assert!(body.contains(r#"class="txn-balance">"#));
    assert!(body.contains(">\u{2014}</td>"));
}
