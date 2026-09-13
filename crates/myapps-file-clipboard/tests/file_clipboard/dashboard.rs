use myapps_file_clipboard::FileClipboardApp;

async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(FileClipboardApp)]).await
}

#[tokio::test]
async fn dashboard_requires_authentication() {
    let app = app().await;
    let r = app.server.get("/file_clipboard").expect_failure().await;
    assert_eq!(r.status_code(), 303);
}

#[tokio::test]
async fn upload_requires_authentication() {
    let app = app().await;
    let r = app
        .server
        .post("/file_clipboard/upload")
        .expect_failure()
        .await;
    assert_eq!(r.status_code(), 303);
}

#[tokio::test]
async fn dashboard_renders_dropzone_and_retention_setting() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/file_clipboard").await.text();
    assert!(body.contains("File Clipboard"), "missing page title");
    assert!(body.contains(r#"id="fc-dropzone""#), "missing drop zone");
    assert!(body.contains(r#"id="fc-file-input""#), "missing file input");
    assert!(
        body.contains(r#"name="retention_days""#),
        "missing retention setting"
    );
    assert!(
        body.contains(r#"value="7""#),
        "retention should default to 7 days"
    );
}

#[tokio::test]
async fn dashboard_shows_empty_state_without_files() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/file_clipboard").await.text();
    assert!(body.contains("No files yet"), "missing empty state");
}

#[tokio::test]
async fn seeding_clears_previous_files() {
    let app = app().await;
    app.seed_and_login(&FileClipboardApp).await;

    // Seeding inserts nothing — an empty clipboard is the honest start state.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let body = app.server.get("/file_clipboard").await.text();
    assert!(body.contains("No files yet"));
}
