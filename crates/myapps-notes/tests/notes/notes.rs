use myapps_notes::NotesApp;

async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(NotesApp)]).await
}

#[tokio::test]
async fn notes_list_requires_authentication() {
    let app = app().await;
    let r = app.server.get("/notes").expect_failure().await;
    assert_eq!(r.status_code(), 303);
}

#[tokio::test]
async fn notes_list_renders() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Notes"));
}

#[tokio::test]
async fn notes_list_shows_seeded_notes() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("Meeting Notes"));
    assert!(body.contains("Rust Tips"));
    assert!(body.contains("Shopping List"));
}

#[tokio::test]
async fn notes_list_shows_pinned_badge() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("notes-pin-badge"));
}

#[tokio::test]
async fn create_note_redirects_to_edit() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.post("/notes/new").expect_failure().await;
    assert_eq!(r.status_code(), 303);
}

#[tokio::test]
async fn edit_note_renders() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("Rust Tips"));
    assert!(body.contains("notes-markdown-editor"));
}

#[tokio::test]
async fn save_note() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app
        .server
        .post(&format!("/notes/{id}/save"))
        .form(&serde_json::json!({
            "title": "Rust Tips Updated",
            "body": "# Updated content",
        }))
        .expect_failure()
        .await;
    assert_eq!(r.status_code(), 303);

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("Rust Tips Updated"));
}

#[tokio::test]
async fn delete_note() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Shopping List' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app
        .server
        .post(&format!("/notes/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(r.status_code(), 303);

    let list = app.server.get("/notes").await;
    let body = list.text();
    assert!(!body.contains("Shopping List"));
}

#[tokio::test]
async fn toggle_pin() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    // Find an unpinned note
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM notes_notes WHERE title = 'Rust Tips' AND pinned = 0 LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let r = app
        .server
        .post(&format!("/notes/{id}/toggle-pin"))
        .expect_failure()
        .await;
    assert_eq!(r.status_code(), 303);

    // Verify it's now pinned
    let (pinned,): (i64,) = sqlx::query_as("SELECT pinned FROM notes_notes WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(pinned, 1);
}

#[tokio::test]
async fn edit_nonexistent_note_shows_not_found() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/notes/99999/edit").await;
    let body = r.text();
    assert!(body.contains("Note not found"));
}

#[tokio::test]
async fn notes_empty_state() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("No notes yet"));
}
