use myapps_notes::NotesApp;

async fn app() -> myapps_test_harness::TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(NotesApp::new())]).await
}

/// Login + insert the four demo rows the UI tests query against by title.
/// The production seed (`services::seed::run`) is a no-op now that title and
/// body both live in the CRDT and we can't realistically serialize markdown
/// → ProseMirror → Yjs from Rust; tests still want predictable rows, so we
/// re-create them here from raw SQL.
async fn setup_with_demos(app: &myapps_test_harness::TestApp) {
    app.seed_and_login(&NotesApp::new()).await;
    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let notes: &[(&str, &str, i64)] = &[
        (
            "Meeting Notes — Project Kickoff",
            "# Kickoff\n\n- Alice, Bob, Carol",
            1,
        ),
        ("Rust Tips", "# Rust\n\nPattern matching tips", 0),
        ("Shopping List", "# Shop\n\n[ ] Tomatoes", 0),
        (
            "Book Notes — Designing Data-Intensive Applications",
            "# DDIA\n\nReliability, scalability",
            1,
        ),
    ];
    for (title, body, pinned) in notes {
        sqlx::query("INSERT INTO notes_notes (user_id, title, body, pinned) VALUES (?, ?, ?, ?)")
            .bind(uid)
            .bind(title)
            .bind(body)
            .bind(pinned)
            .execute(&app.pool)
            .await
            .unwrap();
    }
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
    setup_with_demos(&app).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("notes-grid"));
    assert!(body.contains(r#"action="/notes/new""#));
    assert!(body.contains("btn btn-primary"));
}

#[tokio::test]
async fn notes_list_card_structure() {
    let app = app().await;
    setup_with_demos(&app).await;
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
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("Meeting Notes"));
    assert!(body.contains("Rust Tips"));
    assert!(body.contains("Shopping List"));
}

#[tokio::test]
async fn notes_list_shows_pinned_badge() {
    let app = app().await;
    setup_with_demos(&app).await;
    let r = app.server.get("/notes").await;
    let body = r.text();
    assert!(body.contains("notes-pin-badge"));
    assert!(body.contains("Pinned"));
}

#[tokio::test]
async fn notes_list_shows_preview_text() {
    let app = app().await;
    setup_with_demos(&app).await;
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
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

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
async fn edit_note_has_tiptap_mount() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Tiptap mounts itself into an empty #notes-editor; no contenteditable
    // attribute, no hidden textarea, no body field in the form.
    assert!(body.contains(r#"id="notes-editor""#));
    assert!(!body.contains(r#"contenteditable="true""#));
    assert!(!body.contains(r#"id="notes-raw""#));
    assert!(!body.contains(r#"name="body""#));
}

#[tokio::test]
async fn edit_note_has_editor_css_classes() {
    let app = app().await;
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // No Save endpoint anymore — title is CRDT-bound, denorm flush happens
    // via sendBeacon to /denorm, not via a form submit. Only toggle-pin and
    // delete remain as form actions.
    assert!(!body.contains("/save"));
    assert!(body.contains(&format!(r#"action="/notes/{id}/toggle-pin""#)));
    assert!(body.contains(&format!(r#"action="/notes/{id}/delete""#)));
}

#[tokio::test]
async fn edit_note_no_nested_forms() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // pin and delete are sibling forms, never nested.
    for marker in ["notes-pin-form", "notes-delete-form"] {
        let start = body
            .find(marker)
            .unwrap_or_else(|| panic!("{marker} missing"));
        let end = start + body[start..].find("</form>").unwrap();
        assert!(
            !body[start..end].contains("<form "),
            "{marker} block must not contain a nested form"
        );
    }
}

#[tokio::test]
async fn edit_note_has_delete_form_with_class() {
    let app = app().await;
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Whisper is not configured in tests, so the dictate button must be absent.
    assert!(!body.contains("notes-dictate-btn"));
}

#[tokio::test]
async fn edit_note_has_data_attributes() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id, uuid): (i64, String) =
        sqlx::query_as("SELECT id, client_uuid FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains(r#"data-base=""#));
    assert!(body.contains(&format!(r#"data-client-uuid="{uuid}""#)));
}

#[tokio::test]
async fn edit_note_loads_vendor_bundle_and_bootstrap() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    assert!(body.contains("notes-vendor.bundle.js"));
    assert!(body.contains("notes-tiptap-bootstrap.js"));
    // The legacy contenteditable editor script must not be loaded.
    assert!(!body.contains("notes-editor.js"));
}

#[tokio::test]
async fn denorm_endpoint_updates_title_and_body_and_refreshes_list() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app
        .server
        .post(&format!("/notes/{id}/denorm"))
        .form(&serde_json::json!({
            "title": "Rust Tips — Renamed",
            "body": "# Heading\n\nFreshly synced preview line",
        }))
        .await;
    assert_eq!(r.status_code(), 204);

    let (title, body): (String, String) =
        sqlx::query_as("SELECT title, body FROM notes_notes WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(title, "Rust Tips — Renamed");
    assert_eq!(body, "# Heading\n\nFreshly synced preview line");

    // The list view should pick up both the renamed title and the new
    // preview line (first non-empty non-heading line).
    let list = app.server.get("/notes").await.text();
    assert!(list.contains("Rust Tips — Renamed"));
    assert!(list.contains("Freshly synced preview line"));
}

#[tokio::test]
async fn denorm_endpoint_scopes_to_owner() {
    let app = app().await;
    app.login_as("test", "pass").await;

    // A second user (raw insert — we never authenticate as them, just need a
    // row in `users` to satisfy the FK on `notes_notes.user_id`).
    sqlx::query("INSERT INTO users (username, password_hash) VALUES ('other', 'irrelevant')")
        .execute(&app.pool)
        .await
        .unwrap();
    let (other_uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'other'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let (other_note,): (i64,) = sqlx::query_as(
        "INSERT INTO notes_notes (user_id, client_uuid, title, body) VALUES (?, 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'Theirs', 'orig') RETURNING id",
    )
    .bind(other_uid)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    // Logged in as `test`; POSTing a denorm for the other user's note must
    // be a no-op (WHERE user_id = ? matches zero rows).
    let r = app
        .server
        .post(&format!("/notes/{other_note}/denorm"))
        .form(&serde_json::json!({ "title": "hijacked-title", "body": "hijacked-body" }))
        .await;
    assert_eq!(r.status_code(), 204);

    let (title, body): (String, String) =
        sqlx::query_as("SELECT title, body FROM notes_notes WHERE id = ?")
            .bind(other_note)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(title, "Theirs", "must not overwrite another user's title");
    assert_eq!(body, "orig", "must not overwrite another user's body");
}

#[tokio::test]
async fn delete_note() {
    let app = app().await;
    setup_with_demos(&app).await;

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
    setup_with_demos(&app).await;

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
async fn edit_note_has_crdt_bound_title_input() {
    let app = app().await;
    setup_with_demos(&app).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Rust Tips' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let r = app.server.get(&format!("/notes/{id}/edit")).await;
    let body = r.text();
    // Title now lives in the CRDT and is wired up by the bootstrap; the
    // server-rendered input has no `value=` (would clobber the Y.Text on
    // load) and no `name=` (no form submit). Just the id selector that
    // the bootstrap binds against, and the editor's data-title-input
    // pointer.
    assert!(body.contains(r#"id="notes-title-input""#));
    assert!(body.contains(r##"data-title-input="#notes-title-input""##));
    assert!(!body.contains(r#"name="title""#));
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
    setup_with_demos(&app).await;

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

#[tokio::test]
async fn save_route_is_gone() {
    let app = app().await;
    app.login_as("test", "pass").await;

    sqlx::query("INSERT INTO notes_notes (user_id, title, body) VALUES (1, 'Old', 'orig')")
        .execute(&app.pool)
        .await
        .unwrap();
    let (id,): (i64,) = sqlx::query_as("SELECT id FROM notes_notes WHERE title = 'Old' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // /save used to take a title and redirect; it no longer exists.
    let r = app
        .server
        .post(&format!("/notes/{id}/save"))
        .form(&serde_json::json!({ "title": "New" }))
        .expect_failure()
        .await;
    let s = r.status_code();
    assert!(s == 404 || s == 405, "expected 404/405, got {s}");
    let (title,): (String,) = sqlx::query_as("SELECT title FROM notes_notes WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(title, "Old", "title must not change");
}
