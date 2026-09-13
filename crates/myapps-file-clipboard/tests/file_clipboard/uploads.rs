use axum_test::multipart::{MultipartForm, Part};
use myapps_file_clipboard::{FileClipboardApp, storage};
use myapps_test_harness::TestApp;

async fn app() -> TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(FileClipboardApp)]).await
}

fn part(bytes: Vec<u8>, name: &str) -> Part {
    Part::bytes(bytes)
        .file_name(name.to_string())
        .mime_type("application/octet-stream")
}

async fn upload(app: &TestApp, bytes: Vec<u8>, name: &str) -> axum_test::TestResponse {
    app.server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_part("file", part(bytes, name)))
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
async fn upload_stores_bytes_on_disk_and_metadata_in_sqlite() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let body = upload(&app, b"hello from the laptop".to_vec(), "notes.txt")
        .await
        .text();
    assert!(
        body.contains("notes.txt"),
        "upload should return the new list"
    );

    let uid = user_id(&app, "test").await;
    let (stored, size, original): (String, i64, String) = sqlx::query_as(
        "SELECT stored_name, size_bytes, original_name FROM file_clipboard_files WHERE user_id = ?",
    )
    .bind(uid)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(original, "notes.txt");
    assert_eq!(size, 21);

    let dir = app.file_clipboard_dir.to_string_lossy();
    let path = storage::file_path(&dir, uid, &stored);
    assert_eq!(
        tokio::fs::read(&path).await.unwrap(),
        b"hello from the laptop"
    );
    // The file content is never copied into the row.
    assert_ne!(stored, "notes.txt", "stored name should be a uuid");
}

#[tokio::test]
async fn upload_leaves_no_partial_file_behind() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"done".to_vec(), "a.txt").await;

    let uid = user_id(&app, "test").await;
    let dir = storage::user_dir(&app.file_clipboard_dir.to_string_lossy(), uid);
    let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(!name.ends_with(".part"), "left a partial file: {name}");
    }
}

#[tokio::test]
async fn upload_rejects_a_file_over_the_per_file_limit() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // The harness caps a single file at 5 MB.
    let big = vec![b'x'; 6 * 1024 * 1024];
    let response = app
        .server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_part("file", part(big, "big.bin")))
        .expect_failure()
        .await;

    assert_eq!(response.status_code(), 413);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rejected upload should not be recorded");

    // And the bytes it did stream must not be left on disk.
    let uid = user_id(&app, "test").await;
    let dir = storage::user_dir(&app.file_clipboard_dir.to_string_lossy(), uid);
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        let mut found = 0;
        while let Some(_e) = entries.next_entry().await.unwrap() {
            found += 1;
        }
        assert_eq!(found, 0, "rejected upload left bytes on disk");
    }
}

#[tokio::test]
async fn upload_rejects_an_empty_file() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_part("file", part(Vec::new(), "empty.txt")))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
}

#[tokio::test]
async fn upload_rejects_a_request_with_no_file_field() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_text("note", "no file here"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
}

#[tokio::test]
async fn upload_enforces_the_per_user_quota() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // Harness quota is 20 MB, per-file limit 5 MB: five 4.5 MB files exceed it.
    let chunk = vec![b'y'; 4_500_000];
    for i in 0..4 {
        let r = upload(&app, chunk.clone(), &format!("f{i}.bin")).await;
        assert_eq!(r.status_code(), 200, "file {i} should fit under the quota");
    }

    let response = app
        .server
        .post("/file_clipboard/upload")
        .multipart(MultipartForm::new().add_part("file", part(chunk, "over.bin")))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 413, "quota should be enforced");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
}

#[tokio::test]
async fn file_names_are_escaped_in_the_list() {
    let app = app().await;
    app.login_as("test", "pass").await;

    upload(&app, b"x".to_vec(), r#"<img src=x onerror=alert(1)>.txt"#).await;

    let body = app.server.get("/file_clipboard/files/list").await.text();
    assert!(
        !body.contains("<img src=x"),
        "unescaped file name reached the page"
    );
    assert!(body.contains("&lt;img src=x"), "expected escaped name");
}

#[tokio::test]
async fn a_path_in_the_file_name_cannot_escape_the_user_directory() {
    let app = app().await;
    app.login_as("test", "pass").await;

    upload(&app, b"traversal".to_vec(), "../../../../etc/passwd").await;

    let uid = user_id(&app, "test").await;
    let original: String =
        sqlx::query_scalar("SELECT original_name FROM file_clipboard_files WHERE user_id = ?")
            .bind(uid)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(original, "passwd", "name should be reduced to a basename");

    // Bytes land inside the user's own directory regardless.
    let dir = storage::user_dir(&app.file_clipboard_dir.to_string_lossy(), uid);
    let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
    let mut count = 0;
    while let Some(_e) = entries.next_entry().await.unwrap() {
        count += 1;
    }
    assert_eq!(count, 1);
}

// ── Download ────────────────────────────────────────────────

#[tokio::test]
async fn download_returns_the_bytes_as_an_inert_attachment() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"<html>not rendered</html>".to_vec(), "page.html").await;

    let id: i64 = sqlx::query_scalar("SELECT id FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .await;

    assert_eq!(response.as_bytes(), b"<html>not rendered</html>".as_slice());

    let headers = response.headers();
    assert_eq!(
        headers.get("content-type").unwrap(),
        "application/octet-stream",
        "user bytes must never be served with their own content type"
    );
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
    let disposition = headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        disposition.starts_with("attachment;"),
        "must download, never render inline: {disposition}"
    );
    assert!(disposition.contains("page.html"));
}

#[tokio::test]
async fn download_supports_range_requests() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"0123456789".to_vec(), "digits.bin").await;

    let id: i64 = sqlx::query_scalar("SELECT id FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .add_header("range", "bytes=4-6")
        .await;

    assert_eq!(
        response.status_code(),
        206,
        "a dropped download must resume"
    );
    assert_eq!(response.as_bytes(), b"456".as_slice());
}

#[tokio::test]
async fn download_of_another_users_file_is_not_found() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"mine".to_vec(), "mine.txt").await;

    let other = myapps_core::auth::create_user(&app.pool, "other", "pass")
        .await
        .unwrap();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO file_clipboard_files
             (user_id, original_name, stored_name, size_bytes, expires_at)
         VALUES (?, 'secret.txt', 'deadbeef', 4, datetime('now', '+7 days')) RETURNING id",
    )
    .bind(other)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
}

// ── Delete ──────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_the_row_and_the_bytes() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"delete me".to_vec(), "temp.txt").await;

    let uid = user_id(&app, "test").await;
    let (id, stored): (i64, String) =
        sqlx::query_as("SELECT id, stored_name FROM file_clipboard_files WHERE user_id = ?")
            .bind(uid)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();
    let path = storage::file_path(&dir, uid, &stored);
    assert!(path.exists());

    app.server
        .post(&format!("/file_clipboard/files/{id}/delete"))
        .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(!path.exists(), "bytes should be gone too");
}

#[tokio::test]
async fn delete_cannot_touch_another_users_file() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let other = myapps_core::auth::create_user(&app.pool, "other", "pass")
        .await
        .unwrap();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO file_clipboard_files
             (user_id, original_name, stored_name, size_bytes, expires_at)
         VALUES (?, 'theirs.txt', 'cafebabe', 4, datetime('now', '+7 days')) RETURNING id",
    )
    .bind(other)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    app.server
        .post(&format!("/file_clipboard/files/{id}/delete"))
        .await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_clipboard_files WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "another user's file must survive");
}

#[tokio::test]
async fn upload_accepts_several_files_in_one_request() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // Dropping a selection of files sends them as repeated `file` fields.
    let body = app
        .server
        .post("/file_clipboard/upload")
        .multipart(
            MultipartForm::new()
                .add_part("file", part(b"one".to_vec(), "one.txt"))
                .add_part("file", part(b"two".to_vec(), "two.txt")),
        )
        .await
        .text();

    assert!(body.contains("one.txt"));
    assert!(body.contains("two.txt"));

    let names: Vec<String> =
        sqlx::query_scalar("SELECT original_name FROM file_clipboard_files ORDER BY id")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    assert_eq!(names, vec!["one.txt".to_string(), "two.txt".to_string()]);

    let uid = user_id(&app, "test").await;
    let dir = storage::user_dir(&app.file_clipboard_dir.to_string_lossy(), uid);
    let mut entries = tokio::fs::read_dir(&dir).await.unwrap();
    let mut on_disk = 0;
    while let Some(_e) = entries.next_entry().await.unwrap() {
        on_disk += 1;
    }
    assert_eq!(on_disk, 2, "both files should have their own bytes");
}

#[tokio::test]
async fn download_of_an_unknown_file_is_not_found() {
    let app = app().await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .get("/file_clipboard/files/999/download")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn download_of_an_expired_file_is_not_found() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"stale".to_vec(), "stale.txt").await;

    let id: i64 = sqlx::query_scalar("SELECT id FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE file_clipboard_files SET expires_at = datetime('now', '-1 day')")
        .execute(&app.pool)
        .await
        .unwrap();

    // The bytes are still on disk until the sweep runs; the link must not work.
    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn download_of_a_row_whose_bytes_are_gone_is_not_found() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"vanishing".to_vec(), "gone.txt").await;

    let uid = user_id(&app, "test").await;
    let (id, stored): (i64, String) =
        sqlx::query_as("SELECT id, stored_name FROM file_clipboard_files")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // What a half-finished delete or a wiped storage mount leaves behind.
    let dir = app.file_clipboard_dir.to_string_lossy().into_owned();
    tokio::fs::remove_file(storage::file_path(&dir, uid, &stored))
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404, "must not be a 500");
}

#[tokio::test]
async fn download_keeps_a_non_ascii_name_in_a_valid_header() {
    let app = app().await;
    app.login_as("test", "pass").await;
    upload(&app, b"hola".to_vec(), "año nuevo.txt").await;

    let id: i64 = sqlx::query_scalar("SELECT id FROM file_clipboard_files")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/file_clipboard/files/{id}/download"))
        .await;
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    assert!(disposition.starts_with("attachment;"));
    assert!(
        disposition.contains("filename*=UTF-8''a%C3%B1o%20nuevo.txt"),
        "the UTF-8 form should survive: {disposition}"
    );
    assert!(
        disposition.is_ascii(),
        "a header must stay ASCII or it is dropped: {disposition}"
    );
}

#[tokio::test]
async fn delete_of_an_unknown_file_still_redraws_the_list() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // A stale tab clicking delete twice must not produce an error page.
    let response = app.server.post("/file_clipboard/files/999/delete").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().contains("No files yet"));
}
