#[tokio::test]
async fn labels_page_renders_seeded_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("Groceries"));
    assert!(body.contains("Subscriptions"));
    assert!(body.contains("Entertainment"));
}

#[tokio::test]
async fn create_label_appears_in_list() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("test", "pass").await;

    // Create a label (POST redirects with 303)
    app.server
        .post("/leanfin/labels/create")
        .form(&serde_json::json!({"name": "TestLabel", "color": "#FF0000"}))
        .expect_failure()
        .await;

    // Verify it shows up
    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("TestLabel"));
}

#[tokio::test]
async fn delete_label_removes_from_list() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Entertainment'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label_id}/delete"))
        .expect_failure()
        .await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(!body.contains("Entertainment"));
}

#[tokio::test]
async fn edit_label_updates_name() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    app.server
        .post(&format!("/leanfin/labels/{label_id}/edit"))
        .form(&serde_json::json!({"name": "Food & Groceries", "color": "#4CAF50"}))
        .expect_failure()
        .await;

    let response = app.server.get("/leanfin/labels").await;
    let body = response.text();
    assert!(body.contains("Food &amp; Groceries") || body.contains("Food & Groceries"));
}

#[tokio::test]
async fn label_rules_panel_loads() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/labels/{label_id}/rules"))
        .await;
    let body = response.text();
    // Seeded rules for Groceries: counterparty=Mercadona, counterparty=Carrefour
    assert!(body.contains("Mercadona"));
    assert!(body.contains("Carrefour"));
}

#[tokio::test]
async fn create_rule_for_label() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/labels/{label_id}/rules/create"))
        .form(&serde_json::json!({
            "field": "counterparty",
            "pattern": "Lidl",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Lidl"));
}
