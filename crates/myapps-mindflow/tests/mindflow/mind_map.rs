#[tokio::test]
async fn mind_map_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    let response = app.server.get("/mindflow").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn mind_map_page_renders_with_capture_form() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains(r#"hx-post="/mindflow/capture""#));
    assert!(body.contains("Mind Map"));
}

#[tokio::test]
async fn mind_map_page_shows_category_dropdown() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("Work"));
    assert!(body.contains("Health"));
    assert!(body.contains("Finance"));
}

#[tokio::test]
async fn mind_map_page_has_navigation() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    assert!(body.contains("/mindflow/inbox"));
    assert!(body.contains("/mindflow/actions"));
    assert!(body.contains("/mindflow/categories"));
}

#[tokio::test]
async fn map_data_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    let response = app.server.get("/mindflow/map-data").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn map_data_endpoint_returns_json_with_nodes_and_links() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow/map-data").await;
    let body = response.text();
    assert!(body.contains("nodes"));
    assert!(body.contains("links"));
    // Should contain seeded category names
    assert!(body.contains("Work"));
    assert!(body.contains("Health"));
}

#[tokio::test]
async fn capture_thought_returns_feedback() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app
        .server
        .post("/mindflow/capture")
        .form(&serde_json::json!({
            "content": "A brand new thought",
            "category_id": "",
            "parent_thought_id": "",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Captured!"));
}

#[tokio::test]
async fn capture_thought_with_category() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let (cat_id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Work' AND user_id = (SELECT id FROM users WHERE username = 'seeduser')")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post("/mindflow/capture")
        .form(&serde_json::json!({
            "content": "Work thought",
            "category_id": cat_id.to_string(),
            "parent_thought_id": "",
        }))
        .await;
    let body = response.text();
    assert!(body.contains("Captured!"));

    // Verify it was stored in the correct category
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mindflow_thoughts WHERE content = 'Work thought' AND category_id = ?",
    )
    .bind(cat_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn mind_map_page_shows_inbox_badge() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow").await;
    let body = response.text();
    // Seed data has 3 uncategorized (inbox) thoughts
    assert!(body.contains("Inbox"));
}
