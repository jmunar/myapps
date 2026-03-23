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
async fn notes_list_has_nav_elements() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("<nav"));
    assert!(body.contains("/notes"));
    assert!(body.contains("/logout"));
}

#[tokio::test]
async fn notes_list_has_grid_and_new_note_form() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("notes-grid"));
    assert!(body.contains(r#"action="/notes/new""#));
    assert!(body.contains("btn btn-primary"));
}

#[tokio::test]
async fn notes_list_card_structure() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("notes-card"));
    assert!(body.contains("notes-card-header"));
    assert!(body.contains("notes-card-title"));
    assert!(body.contains("notes-card-date"));
    assert!(body.contains("notes-card-preview"));
}

#[tokio::test]
async fn notes_list_card_links_to_edit() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains(&format!("/notes/{id}/edit")));
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
    assert!(body.contains("Pinned"));
}

#[tokio::test]
async fn notes_list_shows_preview_text() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    // The preview extracts the first non-empty, non-heading line.
    // For "Shopping List", the first such line is a checkbox item.
    // For "Meeting Notes", the first non-heading line is "- Alice, Bob, Carol"
    assert!(body.contains("notes-card-preview"));
}

#[tokio::test]
async fn notes_list_untitled_note_display() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // Insert a note with an empty title
    sqlx::query("INSERT INTO notes_notes (user_id, title, body) VALUES (1, '', 'some body')")
        .execute(&app.pool)
        .await
        .unwrap();

    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("Untitled"));
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
async fn edit_note_has_full_page_structure() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("<nav"));
    assert!(body.contains("/logout"));
}

#[tokio::test]
async fn edit_note_has_contenteditable_and_hidden_textarea() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(r#"contenteditable="true""#));
    assert!(body.contains(r#"id="notes-editor""#));
    assert!(body.contains(r#"id="notes-raw""#));
    assert!(body.contains(r#"name="body""#));
    assert!(body.contains(r#"style="display:none""#));
}

#[tokio::test]
async fn edit_note_has_editor_css_classes() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("notes-editor-container"));
    assert!(body.contains("notes-editor-toolbar"));
    assert!(body.contains("notes-editor-actions"));
    assert!(body.contains("notes-editor-body"));
    assert!(body.contains("notes-title-input"));
    assert!(body.contains("notes-markdown-editor"));
}

#[tokio::test]
async fn edit_note_form_actions_point_to_correct_urls() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(&format!(r#"action="/notes/{id}/save""#)));
    assert!(body.contains(&format!(r#"action="/notes/{id}/delete""#)));
    assert!(body.contains(&format!(r#"action="/notes/{id}/toggle-pin""#)));
}

#[tokio::test]
async fn edit_note_has_delete_form_with_class() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("notes-delete-form"));
    assert!(body.contains("btn btn-danger"));
}

#[tokio::test]
async fn edit_note_has_back_link() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(r#"href="/notes""#));
    assert!(body.contains("Back to notes"));
}

#[tokio::test]
async fn edit_unpinned_note_shows_pin_button() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    // Rust Tips is unpinned in seed data
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM notes_notes WHERE title = 'Rust Tips' AND pinned = 0 LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Should show "Pin" button (not "Unpin")
    assert!(body.contains(">Pin<"));
}

#[tokio::test]
async fn edit_pinned_note_shows_unpin_button() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    // "Meeting Notes" is pinned in seed data
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM notes_notes WHERE title LIKE 'Meeting Notes%' AND pinned = 1 LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Should show "Unpin" button
    assert!(body.contains(">Unpin<"));
}

#[tokio::test]
async fn edit_note_no_dictate_button_without_whisper() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Whisper is not configured in tests, so dictate button should be absent
    assert!(!body.contains("notes-dictate-btn"));
    assert!(body.contains(r#"data-whisper="false""#));
}

#[tokio::test]
async fn edit_note_has_data_attributes() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(&format!(r#"data-note-id="{id}""#)));
    assert!(body.contains(r#"data-base=""#));
}

#[tokio::test]
async fn edit_note_loads_editor_script() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("notes-editor.js"));
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
    assert!(body.contains("empty-state"));
}

#[tokio::test]
async fn edit_note_renders_markdown_content() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    // "Rust Tips" has code blocks, lists, links in its body
    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Code block should be rendered as <pre> with notes-code-block class
    assert!(body.contains("notes-code-block"));
    // List items should be rendered
    assert!(body.contains("<li>"));
    // The raw body should be in the hidden textarea
    assert!(body.contains("Pattern Matching"));
}

#[tokio::test]
async fn edit_note_title_in_input_field() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Title should be in the input field value attribute
    assert!(body.contains(r#"value="Rust Tips""#));
    assert!(body.contains(r#"name="title""#));
}

#[tokio::test]
async fn notes_list_page_header() {
    let app = app().await;
    app.login_as("test", "pass").await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("page-header"));
    assert!(body.contains("<h1>"));
    assert!(body.contains("notes-toolbar"));
}

#[tokio::test]
async fn toggle_pin_then_check_edit_shows_unpin() {
    let app = app().await;
    app.seed_and_login(&NotesApp).await;

    // Start with an unpinned note
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM notes_notes WHERE title = 'Rust Tips' AND pinned = 0 LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    // Pin it
    app.server
        .post(&format!("/notes/{id}/toggle-pin"))
        .expect_failure()
        .await;

    // Now the edit page should show "Unpin"
    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(">Unpin<"));
}
