use crate::harness;

#[tokio::test]
async fn actions_page_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/mindflow/actions").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn actions_page_renders_seeded_actions() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let response = app.server.get("/mindflow/actions").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Set up meeting room for Q1 review"));
    assert!(body.contains("Book dentist appointment"));
    assert!(body.contains("Compare 3 electricity providers"));
    assert!(body.contains("Buy birthday present for Alex"));
}

#[tokio::test]
async fn actions_page_shows_priority_badges() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let response = app.server.get("/mindflow/actions").await;
    let body = response.text();
    assert!(body.contains("high"));
    assert!(body.contains("medium"));
    assert!(body.contains("low"));
}

#[tokio::test]
async fn actions_page_empty_state() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/mindflow/actions").await;
    let body = response.text();
    assert!(response.status_code().is_success());
    // No seeded actions for this user
    assert!(!body.contains("Set up meeting room"));
}

#[tokio::test]
async fn toggle_action_status() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_actions WHERE status = 'pending' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/actions/{id}/toggle"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let (status,): (String,) = sqlx::query_as("SELECT status FROM mindflow_actions WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status, "done");
}

#[tokio::test]
async fn toggle_done_action_back_to_pending() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM mindflow_actions WHERE status = 'pending' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Mark as done
    sqlx::query(
        "UPDATE mindflow_actions SET status = 'done', completed_at = datetime('now') WHERE id = ?",
    )
    .bind(id)
    .execute(&app.pool)
    .await
    .unwrap();

    // Toggle back
    app.server
        .post(&format!("/mindflow/actions/{id}/toggle"))
        .expect_failure()
        .await;

    let (status,): (String,) = sqlx::query_as("SELECT status FROM mindflow_actions WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn delete_action() {
    let app = harness::spawn_app().await;
    app.seed_and_login_mindflow().await;

    let (id,): (i64,) = sqlx::query_as("SELECT id FROM mindflow_actions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .post(&format!("/mindflow/actions/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let count: Option<(i64,)> = sqlx::query_as("SELECT id FROM mindflow_actions WHERE id = ?")
        .bind(id)
        .fetch_optional(&app.pool)
        .await
        .unwrap();
    assert!(count.is_none());
}
