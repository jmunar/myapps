use crate::harness;

#[tokio::test]
async fn thought_detail_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app
        .server
        .get("/mindflow/thoughts/1")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn thought_detail_page_renders() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM mindflow_thoughts WHERE content LIKE '%Q1 project%' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app.server.get(&format!("/mindflow/thoughts/{id}")).await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Q1 project"));
    // Should show comments section
    assert!(body.contains("Need to update the timeline"));
    assert!(body.contains("Check with DevOps"));
}

#[tokio::test]
async fn thought_detail_shows_action_form() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) = sqlx::query_as("SELECT id FROM mindflow_thoughts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app.server.get(&format!("/mindflow/thoughts/{id}")).await;
    let body = response.text();
    assert!(body.contains(r#"name="title""#));
    assert!(body.contains(r#"name="priority""#));
}

#[tokio::test]
async fn thought_detail_shows_sub_thoughts() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    // The API redesign thought has sub-thoughts
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM mindflow_thoughts WHERE content LIKE '%API redesign%' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app.server.get(&format!("/mindflow/thoughts/{id}")).await;
    let body = response.text();
    assert!(body.contains("Review current endpoint naming"));
    assert!(body.contains("Benchmark response times"));
    assert!(body.contains("Draft migration guide"));
}

#[tokio::test]
async fn add_comment_to_thought() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) = sqlx::query_as("SELECT id FROM mindflow_thoughts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/thoughts/{id}/comment"))
        .form(&serde_json::json!({"content": "My new comment"}))
        .await;
    let body = response.text();
    assert!(body.contains("My new comment"));
}

#[tokio::test]
async fn archive_thought_toggles_status() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_thoughts WHERE status = 'active' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/thoughts/{id}/archive"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let (status,): (String,) = sqlx::query_as("SELECT status FROM mindflow_thoughts WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status, "archived");
}

#[tokio::test]
async fn recategorize_thought() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    // Get an inbox thought (no category)
    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_thoughts WHERE category_id IS NULL LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (cat_id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_categories WHERE name = 'Personal' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/thoughts/{id}/recategorize"))
        .form(&serde_json::json!({"category_id": cat_id.to_string()}))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let (new_cat,): (Option<i64>,) =
        sqlx::query_as("SELECT category_id FROM mindflow_thoughts WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(new_cat, Some(cat_id));
}

#[tokio::test]
async fn create_action_from_thought() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) = sqlx::query_as("SELECT id FROM mindflow_thoughts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/thoughts/{id}/action"))
        .form(&serde_json::json!({
            "title": "New test action",
            "priority": "high",
            "due_date": "2026-04-01",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM mindflow_actions WHERE title = 'New test action'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn create_sub_thought() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (parent_id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_thoughts WHERE parent_thought_id IS NULL LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/thoughts/{parent_id}/sub-thought"))
        .form(&serde_json::json!({"content": "A nested sub-thought"}))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mindflow_thoughts WHERE parent_thought_id = ? AND content = 'A nested sub-thought'",
    )
    .bind(parent_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
