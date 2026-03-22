#[tokio::test]
async fn inbox_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    let response = app.server.get("/mindflow/inbox").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn inbox_page_renders_uncategorized_thoughts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow/inbox").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    // Seed data has 3 inbox thoughts
    assert!(body.contains("note-taking tool"));
    assert!(body.contains("birthday present for Alex"));
    assert!(body.contains("automate weekly report"));
}

#[tokio::test]
async fn inbox_page_empty_state_when_all_categorized() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.login_as("test", "pass").await;

    // No thoughts at all — should show empty state
    let response = app.server.get("/mindflow/inbox").await;
    let body = response.text();
    assert!(response.status_code().is_success());
    // Should not contain any thought cards
    assert!(!body.contains("bulk-form"));
}

#[tokio::test]
async fn inbox_has_bulk_recategorize_form() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let response = app.server.get("/mindflow/inbox").await;
    let body = response.text();
    assert!(body.contains("bulk-form"));
    assert!(body.contains(r#"name="category_id""#));
    assert!(body.contains(r#"name="thought_ids""#));
}

#[tokio::test]
async fn bulk_recategorize_moves_thoughts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_mindflow::MindFlowApp)]).await;
    app.seed_and_login(&myapps_mindflow::MindFlowApp).await;

    let (cat_id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Personal' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let inbox_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM mindflow_thoughts WHERE category_id IS NULL AND status = 'active'",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();

    let ids_csv = inbox_ids
        .iter()
        .map(|(id,)| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    app.server
        .post("/mindflow/inbox/recategorize")
        .form(&serde_json::json!({
            "category_id": cat_id,
            "thought_ids": ids_csv,
        }))
        .expect_failure()
        .await;

    // Inbox should now be empty
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mindflow_thoughts WHERE category_id IS NULL AND status = 'active'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
