#[tokio::test]
async fn dashboard_requires_authentication() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    let response = app.server.get("/voice").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn dashboard_renders_empty_state() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/voice").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    // Should show new transcription button
    assert!(body.contains("/voice/new"));
}

#[tokio::test]
async fn dashboard_has_navigation() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/voice").await;
    let body = response.text();
    assert!(body.contains("/voice/new"));
    assert!(body.contains("/logout"));
}

#[tokio::test]
async fn dashboard_shows_job_after_insert() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_voice_to_text::VoiceToTextApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO voice_to_text_jobs (user_id, original_filename, audio_path, model_used, status, transcription) VALUES (?, 'test.wav', '/tmp/test.wav', 'base', 'done', 'Test transcription')",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app.server.get("/voice").await;
    let body = response.text();
    assert!(body.contains("test.wav"));
    assert!(body.contains("done"));
}
