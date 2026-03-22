use crate::harness;

#[tokio::test]
async fn new_form_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/voice/new").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn new_form_renders_upload_page() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/voice/new").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("New Transcription"));
    assert!(body.contains(r#"name="audio""#));
    assert!(body.contains(r#"hx-post="/voice/upload""#));
}

#[tokio::test]
async fn new_form_has_recording_section() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/voice/new").await;
    let body = response.text();
    assert!(body.contains("Start Recording") || body.contains("MediaRecorder"));
}

#[tokio::test]
async fn job_detail_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/voice/jobs/1").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn job_detail_renders_for_completed_job() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO voice_to_text_jobs (id, user_id, original_filename, audio_path, model_used, status, transcription, duration_secs) VALUES (100, ?, 'recording.wav', '/tmp/recording.wav', 'base', 'done', 'Hello world transcription', 5.2)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app.server.get("/voice/jobs/100").await;
    let body = response.text();
    assert!(body.contains("recording.wav"));
    assert!(body.contains("Hello world transcription"));
    assert!(body.contains("done"));
}

#[tokio::test]
async fn job_detail_shows_error_for_failed_job() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO voice_to_text_jobs (id, user_id, original_filename, audio_path, model_used, status, error_message) VALUES (101, ?, 'bad.wav', '/tmp/bad.wav', 'base', 'failed', 'Model not found')",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app.server.get("/voice/jobs/101").await;
    let body = response.text();
    assert!(body.contains("failed"));
    assert!(body.contains("Model not found"));
}

#[tokio::test]
async fn jobs_list_partial_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/voice/jobs/list").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn jobs_list_partial_returns_table_rows() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO voice_to_text_jobs (user_id, original_filename, audio_path, model_used, status) VALUES (?, 'partial.wav', '/tmp/partial.wav', 'base', 'pending')",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app.server.get("/voice/jobs/list").await;
    let body = response.text();
    assert!(body.contains("partial.wav"));
    // Should be a table rows fragment, not a full page
    assert!(!body.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn delete_job_removes_from_list() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO voice_to_text_jobs (id, user_id, original_filename, audio_path, model_used, status, transcription) VALUES (200, ?, 'delete-me.wav', '/tmp/nonexistent.wav', 'base', 'done', 'Delete me')",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app.server.post("/voice/jobs/200/delete").await;
    let body = response.text();
    assert!(!body.contains("delete-me.wav"));

    // Verify deleted from DB
    let count: Option<(i64,)> = sqlx::query_as("SELECT id FROM voice_to_text_jobs WHERE id = 200")
        .fetch_optional(&app.pool)
        .await
        .unwrap();
    assert!(count.is_none());
}
