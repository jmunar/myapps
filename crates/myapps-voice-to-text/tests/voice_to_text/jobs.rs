#[tokio::test]
async fn new_form_requires_authentication() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    let response = app.server.get("/voice/new").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn new_form_renders_upload_page() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
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
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/voice/new").await;
    let body = response.text();
    assert!(body.contains("Start Recording") || body.contains("MediaRecorder"));
}

#[tokio::test]
async fn job_detail_requires_authentication() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    let response = app.server.get("/voice/jobs/1").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn job_detail_renders_for_completed_job() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
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
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
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
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    let response = app.server.get("/voice/jobs/list").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn jobs_list_partial_returns_table_rows() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
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
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
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

// recorder.js drives the mic buttons by id and reads its status strings off
// #rec-status; the inline onclick handlers it replaced are gone.
#[tokio::test]
async fn new_job_page_gives_the_recorder_its_hooks() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/voice/new").await.text();
    assert!(body.contains(r#"id="rec-start""#));
    assert!(body.contains(r#"id="rec-stop""#));
    assert!(body.contains("data-recording="));
    assert!(body.contains("data-processing="));
    assert!(!body.contains("onclick=\"startRecording()\""));
}

// The recorder also reaches outside its own box: it reads the model the upload
// form is set to and swaps the response into #rec-result. Both live in markup
// the recorder does not own, so a rename there breaks it silently.
#[tokio::test]
async fn new_job_page_gives_the_recorder_a_model_and_a_result_slot() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/voice/new").await.text();
    assert!(
        body.contains(r#"<select id="model" name="model">"#),
        "recorder.js posts document.getElementById('model').value"
    );
    assert!(
        body.contains(r#"<div id="rec-result">"#),
        "recorder.js writes the upload response into #rec-result"
    );
    // The upload endpoint it posts to is the same one the form uses.
    assert!(body.contains(r#"hx-post="/voice/upload""#));
}
