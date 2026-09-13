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

#[tokio::test]
async fn file_list_partial_is_a_fragment_not_a_page() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/file_clipboard/files/list").await.text();
    assert!(
        !body.contains("<!DOCTYPE"),
        "partial must not be a full page"
    );
    assert!(!body.contains("<nav"), "partial must not repeat the nav");
    assert!(body.contains("No files yet"));
}

#[tokio::test]
async fn every_file_route_requires_authentication() {
    let app = app().await;

    for (method, path) in [
        ("GET", "/file_clipboard/files/list"),
        ("GET", "/file_clipboard/files/1/download"),
        ("POST", "/file_clipboard/files/1/delete"),
        ("POST", "/file_clipboard/settings"),
    ] {
        let r = match method {
            "GET" => app.server.get(path).expect_failure().await,
            _ => app.server.post(path).expect_failure().await,
        };
        assert_eq!(r.status_code(), 303, "{method} {path} should redirect");
    }
}

#[tokio::test]
async fn the_list_shows_a_stored_file_with_its_size_and_usage() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let uid: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO file_clipboard_files
             (user_id, original_name, stored_name, size_bytes, expires_at)
         VALUES (?, 'holiday.zip', 'aaaa-bbbb', 2048, datetime('now', '+7 days'))",
    )
    .bind(uid)
    .execute(&app.pool)
    .await
    .unwrap();

    let body = app.server.get("/file_clipboard/files/list").await.text();
    assert!(body.contains("holiday.zip"));
    assert!(body.contains("2.0 KB"), "size should be human readable");
    assert!(
        body.contains(r#"href="/file_clipboard/files/1/download""#),
        "each row needs a download link"
    );
    assert!(
        body.contains(r#"hx-post="/file_clipboard/files/1/delete""#),
        "each row needs a delete form"
    );
    assert!(
        body.contains("2.0 KB / 20.0 MB used"),
        "the footer should show usage against the quota: {body}"
    );
}

#[tokio::test]
async fn the_list_shows_only_the_signed_in_users_files() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let other = myapps_core::auth::create_user(&app.pool, "other", "pass")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO file_clipboard_files
             (user_id, original_name, stored_name, size_bytes, expires_at)
         VALUES (?, 'theirs.txt', 'cafed00d', 10, datetime('now', '+7 days'))",
    )
    .bind(other)
    .execute(&app.pool)
    .await
    .unwrap();

    let body = app.server.get("/file_clipboard/files/list").await.text();
    assert!(
        !body.contains("theirs.txt"),
        "clipboards must not be shared"
    );
    assert!(body.contains("No files yet"));
}
