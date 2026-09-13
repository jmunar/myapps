use axum_test::multipart::{MultipartForm, Part};
use myapps_file_clipboard::{FileClipboardApp, services, storage};
use myapps_test_harness::TestApp;

async fn app() -> TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(FileClipboardApp)]).await
}

async fn upload(app: &TestApp, name: &str) -> axum_test::TestResponse {
    app.server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_part(
            "file",
            Part::bytes(b"payload".to_vec()).file_name(name.to_string()),
        ))
        .await
}

async fn user_id(app: &TestApp, username: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn uploads_expire_after_the_default_seven_days() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "a.txt").await;

    let days: i64 = sqlx::query_scalar(
        "SELECT CAST(julianday(expires_at) - julianday(created_at) + 0.5 AS INTEGER)
         FROM file_clipboard_files",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(days, 7);
}

#[tokio::test]
async fn changing_the_period_restamps_existing_files() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "a.txt").await;

    let response = app
        .server
        .post("/file_clipboard/settings")
        .form(&serde_json::json!({ "retention_days": 30 }))
        .await;
    assert_eq!(
        response.headers().get("hx-trigger").unwrap(),
        "fcRefresh",
        "the list must refresh so expiry dates stay truthful"
    );

    let days: i64 = sqlx::query_scalar(
        "SELECT CAST(julianday(expires_at) - julianday(created_at) + 0.5 AS INTEGER)
         FROM file_clipboard_files",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(days, 30, "existing files should follow the new period");

    // And the setting applies to later uploads.
    upload(&app, "b.txt").await;
    let all: Vec<i64> = sqlx::query_scalar(
        "SELECT CAST(julianday(expires_at) - julianday(created_at) + 0.5 AS INTEGER)
         FROM file_clipboard_files",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    assert_eq!(all, vec![30, 30]);
}

#[tokio::test]
async fn retention_period_is_bounded() {
    let app = app().await;
    app.login_as("test", "pass").await;

    for bad in [0, -5, 400] {
        let response = app
            .server
            .post("/file_clipboard/settings")
            .form(&serde_json::json!({ "retention_days": bad }))
            .expect_failure()
            .await;
        assert_eq!(response.status_code(), 400, "{bad} days should be rejected");
    }

    let stored: Option<i64> =
        sqlx::query_scalar("SELECT retention_days FROM file_clipboard_settings")
            .fetch_optional(&app.pool)
            .await
            .unwrap();
    assert!(stored.is_none(), "invalid values must not be stored");
}

#[tokio::test]
async fn expired_files_are_hidden_before_the_sweep_runs() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "stale.txt").await;

    sqlx::query("UPDATE file_clipboard_files SET expires_at = datetime('now', '-1 day')")
        .execute(&app.pool)
        .await
        .unwrap();

    let body = app.server.get("/file_clipboard/files/list").await.text();
    assert!(
        !body.contains("stale.txt"),
        "an expired file must not be listed even if the sweep has not run"
    );
}

#[tokio::test]
async fn sweep_deletes_expired_files_and_their_bytes() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "stale.txt").await;
    upload(&app, "fresh.txt").await;

    let uid = user_id(&app, "test").await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();

    let stale: String = sqlx::query_scalar(
        "SELECT stored_name FROM file_clipboard_files WHERE original_name = 'stale.txt'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE file_clipboard_files SET expires_at = datetime('now', '-1 day')
         WHERE original_name = 'stale.txt'",
    )
    .execute(&app.pool)
    .await
    .unwrap();

    services::retention::sweep(&app.pool, &dir).await.unwrap();

    let remaining: Vec<String> =
        sqlx::query_scalar("SELECT original_name FROM file_clipboard_files")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(remaining, vec!["fresh.txt".to_string()]);
    assert!(
        !storage::file_path(&dir, uid, &stale).exists(),
        "expired bytes should be reclaimed"
    );
}

#[tokio::test]
async fn sweep_reclaims_bytes_left_by_a_deleted_row() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "orphan.txt").await;

    let uid = user_id(&app, "test").await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();
    let stored: String = sqlx::query_scalar("SELECT stored_name FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // What `delete-user` and `delete_user_app_data` do: rows vanish, bytes stay.
    sqlx::query("DELETE FROM file_clipboard_files")
        .execute(&app.pool)
        .await
        .unwrap();
    let path = storage::file_path(&dir, uid, &stored);
    assert!(path.exists(), "precondition: bytes outlive the row");

    services::retention::sweep(&app.pool, &dir).await.unwrap();

    assert!(!path.exists(), "orphaned bytes should be reclaimed");
}

#[tokio::test]
async fn sweep_leaves_a_partial_upload_in_flight_alone() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let uid = user_id(&app, "test").await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();

    let user_dir = storage::user_dir(&dir, uid);
    tokio::fs::create_dir_all(&user_dir).await.unwrap();
    let part = user_dir.join("11111111-2222-3333-4444-555555555555.part");
    tokio::fs::write(&part, b"still uploading").await.unwrap();

    services::retention::sweep(&app.pool, &dir).await.unwrap();

    assert!(
        part.exists(),
        "a fresh .part file may still be an upload in progress"
    );
}

#[tokio::test]
async fn sweep_is_safe_to_run_on_an_empty_deployment() {
    let app = app().await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();
    services::retention::sweep(&app.pool, &dir).await.unwrap();
}

#[tokio::test]
async fn the_saved_period_comes_back_on_the_dashboard() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let saved = app
        .server
        .post("/file_clipboard/settings")
        .form(&serde_json::json!({ "retention_days": 30 }))
        .await
        .text();
    assert!(saved.contains("Saved."), "expected a confirmation: {saved}");

    let body = app.server.get("/file_clipboard").await.text();
    assert!(
        body.contains(r#"value="30""#),
        "the form should show the stored period, not the default"
    );
}

#[tokio::test]
async fn the_bounds_themselves_are_accepted() {
    let app = app().await;
    app.login_as("test", "pass").await;

    for days in [1, 365] {
        let response = app
            .server
            .post("/file_clipboard/settings")
            .form(&serde_json::json!({ "retention_days": days }))
            .await;
        assert_eq!(response.status_code(), 200, "{days} days should be allowed");

        let stored: i64 = sqlx::query_scalar("SELECT retention_days FROM file_clipboard_settings")
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(stored, days);
    }
}

#[tokio::test]
async fn sweep_collects_an_abandoned_partial_upload() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let uid = user_id(&app, "test").await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();

    let user_dir = storage::user_dir(&dir, uid);
    tokio::fs::create_dir_all(&user_dir).await.unwrap();
    let part = user_dir.join("99999999-8888-7777-6666-555555555555.part");
    tokio::fs::write(&part, b"never finished").await.unwrap();

    // Older than the grace period: the upload that wrote it is long gone.
    let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 60 * 60);
    let handle = std::fs::File::options().write(true).open(&part).unwrap();
    handle
        .set_times(std::fs::FileTimes::new().set_modified(stale))
        .unwrap();
    drop(handle);

    services::retention::sweep(&app.pool, &dir).await.unwrap();

    assert!(!part.exists(), "an abandoned .part should be reclaimed");
}

#[tokio::test]
async fn sweep_removes_an_emptied_user_directory() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, "only.txt").await;

    let uid = user_id(&app, "test").await;
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();
    let user_dir = storage::user_dir(&dir, uid);
    assert!(
        user_dir.exists(),
        "precondition: the upload made a directory"
    );

    let id: i64 = sqlx::query_scalar("SELECT id FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    app.server
        .post(&format!("/file_clipboard/files/{id}/delete"))
        .await;

    services::retention::sweep(&app.pool, &dir).await.unwrap();

    assert!(
        !user_dir.exists(),
        "an empty per-user directory should not be left behind"
    );
}
