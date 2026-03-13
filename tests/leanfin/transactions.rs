use crate::harness;

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
