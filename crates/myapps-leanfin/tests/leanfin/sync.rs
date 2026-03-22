#[tokio::test]
async fn dashboard_shows_sync_button() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();
    assert!(body.contains(r#"id="sync-container"#));
    assert!(body.contains("sync-container"));
    assert!(body.contains(r#"hx-post="/leanfin/sync"#));
    assert!(body.contains("sync-icon"));
}

#[tokio::test]
async fn accounts_page_shows_sync_button() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains(r#"id="sync-container"#));
    assert!(body.contains(r#"hx-post="/leanfin/sync"#));
    assert!(body.contains("sync-icon"));
}

#[tokio::test]
async fn sync_endpoint_requires_auth() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;

    let response = app.server.post("/leanfin/sync").expect_failure().await;
    // Unauthenticated requests redirect to login
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn sync_with_no_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("user", "pass").await;

    let response = app.server.post("/leanfin/sync").await;
    let body = response.text();
    assert!(body.contains("No accounts to sync"));
    assert!(body.contains("sync-status-ok"));
}

#[tokio::test]
async fn sync_with_seeded_accounts_no_credentials() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Seeded user has no Enable Banking credentials configured, so sync skips gracefully
    let response = app.server.post("/leanfin/sync").await;
    let body = response.text();
    assert!(body.contains("sync-status-ok"));
    assert!(body.contains("No accounts to sync"));
}

#[tokio::test]
async fn sync_response_includes_hx_trigger_header() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("user", "pass").await;

    let response = app.server.post("/leanfin/sync").await;
    let binding = response.header("HX-Trigger");
    let hx_trigger = binding.to_str().unwrap();
    assert_eq!(hx_trigger, "sync-done");
}

#[tokio::test]
async fn dashboard_txn_table_has_sync_done_trigger() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();
    assert!(body.contains(r#"hx-trigger="load, sync-done from:body"#));
}
