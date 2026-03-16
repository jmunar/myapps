mod harness;

#[tokio::test]
async fn unauthenticated_request_redirects_to_login() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn login_page_renders() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/login").await;
    let body = response.text();
    assert!(body.contains("MyApps"));
    assert!(body.contains(r#"name="username""#));
    assert!(body.contains(r#"name="password""#));
}

#[tokio::test]
async fn login_with_valid_credentials_redirects() {
    let app = harness::spawn_app().await;
    myapps::auth::create_user(&app.pool, "test", "pass")
        .await
        .unwrap();

    let response = app
        .server
        .post("/login")
        .form(&serde_json::json!({"username": "test", "password": "pass"}))
        .expect_failure()
        .await;

    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn login_with_wrong_password_shows_error() {
    let app = harness::spawn_app().await;
    myapps::auth::create_user(&app.pool, "test", "pass")
        .await
        .unwrap();

    let response = app
        .server
        .post("/login")
        .form(&serde_json::json!({"username": "test", "password": "wrong"}))
        .await;

    let body = response.text();
    assert!(body.contains("Invalid credentials"));
}

#[tokio::test]
async fn authenticated_user_sees_launcher() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/").await;
    let body = response.text();
    assert!(body.contains("LeanFin"));
}

#[tokio::test]
async fn logout_clears_session() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Logout (redirects)
    app.server.get("/logout").expect_failure().await;

    // Protected route should redirect to login again
    let response = app.server.get("/").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

// --- Launcher app visibility tests (FEAT-26) ---

#[tokio::test]
async fn launcher_shows_all_three_apps_by_default() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/").await;
    let body = response.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(body.contains("VoiceToText"));
}

#[tokio::test]
async fn launcher_has_config_button() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/").await;
    let body = response.text();
    assert!(body.contains("launcher-edit-btn"));
    assert!(body.contains(r#"hx-get="/launcher/edit""#));
}

#[tokio::test]
async fn edit_mode_shows_all_apps_with_toggle_buttons() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/launcher/edit").await;
    let body = response.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(body.contains("VoiceToText"));
    assert!(body.contains("launcher-toggle"));
    assert!(body.contains("Toggle app visibility"));
}

#[tokio::test]
async fn edit_mode_has_done_button() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/launcher/edit").await;
    let body = response.text();
    assert!(body.contains("launcher-done-btn"));
    assert!(body.contains(r#"hx-get="/launcher/grid""#));
    assert!(body.contains("Done"));
}

#[tokio::test]
async fn hiding_app_removes_it_from_launcher() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Hide LeanFin
    app.server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "leanfin", "visible": "0"}))
        .await;

    // Launcher should no longer show LeanFin
    let response = app.server.get("/").await;
    let body = response.text();
    assert!(
        !body.contains("launcher-card\" "),
        "LeanFin card should not appear as visible link"
    );
    assert!(body.contains("MindFlow"));
    assert!(body.contains("VoiceToText"));
    // LeanFin should not appear as a clickable launcher card
    assert!(!body.contains(r#"href="/leanfin""#));
}

#[tokio::test]
async fn hiding_all_apps_shows_empty_state() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Hide all three apps
    for key in &["leanfin", "mindflow", "voice_to_text"] {
        app.server
            .post("/launcher/visibility")
            .form(&serde_json::json!({"app_key": key, "visible": "0"}))
            .await;
    }

    let response = app.server.get("/").await;
    let body = response.text();
    assert!(body.contains("No apps visible"));
}

#[tokio::test]
async fn invalid_app_key_is_ignored() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // POST with a bogus app_key should not error
    let response = app
        .server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "nonexistent", "visible": "0"}))
        .await;

    // Should still return the edit mode view successfully
    let body = response.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("Toggle app visibility"));
}

#[tokio::test]
async fn edit_mode_shows_hidden_apps_with_hidden_class() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Hide MindFlow
    app.server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "mindflow", "visible": "0"}))
        .await;

    let response = app.server.get("/launcher/edit").await;
    let body = response.text();
    // MindFlow card should have the "hidden" class
    assert!(body.contains(r#"launcher-card-edit hidden"#));
    // LeanFin card should NOT have the "hidden" class
    assert!(body.contains(r#"id="card-leanfin""#));
    assert!(body.contains(r#"id="card-mindflow""#));
}

#[tokio::test]
async fn grid_fragment_returns_normal_mode() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/launcher/grid").await;
    let body = response.text();
    // Normal mode header with config button, not edit mode
    assert!(body.contains("launcher-edit-btn"));
    assert!(body.contains("Choose an application"));
    assert!(!body.contains("Toggle app visibility"));
}

#[tokio::test]
async fn rehiding_then_showing_app_restores_visibility() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    // Hide LeanFin
    app.server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "leanfin", "visible": "0"}))
        .await;

    // Show LeanFin again
    app.server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "leanfin", "visible": "1"}))
        .await;

    let response = app.server.get("/").await;
    let body = response.text();
    assert!(body.contains(r#"href="/leanfin""#));
}
