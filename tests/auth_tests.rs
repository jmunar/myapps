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

    // Hide all apps
    for key in &[
        "leanfin",
        "mindflow",
        "voice_to_text",
        "classroom_input",
        "notes",
    ] {
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

// --- DEPLOY_APPS subset tests (FEAT-33) ---

#[tokio::test]
async fn deploy_apps_none_shows_all_apps_in_launcher() {
    let app = harness::spawn_app_with_deploy_apps(None).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/").await.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(body.contains("VoiceToText"));
    assert!(body.contains("ClassroomInput"));
}

#[tokio::test]
async fn deploy_apps_subset_only_shows_selected_apps_in_launcher() {
    let app =
        harness::spawn_app_with_deploy_apps(Some(vec!["leanfin".into(), "mindflow".into()])).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/").await.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(!body.contains("VoiceToText"));
    assert!(!body.contains("ClassroomInput"));
}

#[tokio::test]
async fn deploy_apps_subset_excluded_app_route_returns_404() {
    let app = harness::spawn_app_with_deploy_apps(Some(vec!["leanfin".into()])).await;
    app.login_as("test", "pass").await;

    // LeanFin is deployed — should be reachable
    let response = app.server.get("/leanfin").await;
    assert_eq!(response.status_code(), 200);

    // MindFlow is NOT deployed — should 404
    let response = app.server.get("/mindflow").expect_failure().await;
    assert_eq!(response.status_code(), 404);

    // VoiceToText is NOT deployed — should 404
    let response = app.server.get("/voice").expect_failure().await;
    assert_eq!(response.status_code(), 404);

    // ClassroomInput is NOT deployed — should 404
    let response = app.server.get("/classroom").expect_failure().await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn deploy_apps_single_app_only_mounts_that_app() {
    let app = harness::spawn_app_with_deploy_apps(Some(vec!["classroom_input".into()])).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/").await.text();
    assert!(body.contains("ClassroomInput"));
    assert!(!body.contains("LeanFin"));
    assert!(!body.contains("MindFlow"));
    assert!(!body.contains("VoiceToText"));

    // Classroom route works
    let response = app.server.get("/classroom").await;
    assert_eq!(response.status_code(), 200);

    // Others 404
    let response = app.server.get("/leanfin").expect_failure().await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn deploy_apps_edit_mode_only_shows_deployed_apps() {
    let app =
        harness::spawn_app_with_deploy_apps(Some(vec!["leanfin".into(), "mindflow".into()])).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/launcher/edit").await.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(!body.contains("VoiceToText"));
    assert!(!body.contains("ClassroomInput"));
}

#[tokio::test]
async fn deploy_apps_empty_vec_shows_no_apps() {
    let app = harness::spawn_app_with_deploy_apps(Some(vec![])).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/").await.text();
    assert!(body.contains("No apps visible"));
    assert!(!body.contains("LeanFin"));
    assert!(!body.contains("MindFlow"));
}

#[tokio::test]
async fn deploy_apps_visibility_toggle_only_applies_to_deployed_apps() {
    let app =
        harness::spawn_app_with_deploy_apps(Some(vec!["leanfin".into(), "mindflow".into()])).await;
    app.login_as("test", "pass").await;

    // Hiding a non-deployed app key should be ignored
    app.server
        .post("/launcher/visibility")
        .form(&serde_json::json!({"app_key": "voice_to_text", "visible": "0"}))
        .await;

    // Launcher should still show the two deployed apps
    let body = app.server.get("/").await.text();
    assert!(body.contains("LeanFin"));
    assert!(body.contains("MindFlow"));
    assert!(!body.contains("VoiceToText"));
}

// --- Language settings tests ---

#[tokio::test]
async fn set_language_redirects() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/settings/language")
        .form(&serde_json::json!({"language": "es", "redirect": "/"}))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn set_language_persists_in_db() {
    let app = harness::spawn_app().await;
    app.login_as("test", "pass").await;

    app.server
        .post("/settings/language")
        .form(&serde_json::json!({"language": "es", "redirect": "/"}))
        .expect_failure()
        .await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let (lang,): (String,) = sqlx::query_as("SELECT language FROM user_settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(lang, "es");
}

#[tokio::test]
async fn set_language_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app
        .server
        .post("/settings/language")
        .form(&serde_json::json!({"language": "es"}))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

// --- Invite tests ---

#[tokio::test]
async fn invite_page_renders_for_valid_token() {
    let app = harness::spawn_app().await;
    let token = myapps::auth::create_invite(&app.pool).await.unwrap();

    let response = app.server.get(&format!("/invite/{token}")).await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains(r#"name="username""#));
    assert!(body.contains(r#"name="password""#));
    assert!(body.contains(r#"name="confirm_password""#));
}

#[tokio::test]
async fn invite_page_shows_error_for_invalid_token() {
    let app = harness::spawn_app().await;

    let response = app.server.get("/invite/bogus_token_123").await;
    let body = response.text();
    // Should show error, not a form
    assert!(!body.contains(r#"name="username""#));
    assert!(body.contains("login"));
}

#[tokio::test]
async fn invite_registration_creates_user_and_logs_in() {
    let app = harness::spawn_app().await;
    let token = myapps::auth::create_invite(&app.pool).await.unwrap();

    let response = app
        .server
        .post(&format!("/invite/{token}"))
        .form(&serde_json::json!({
            "username": "newuser",
            "password": "secret123",
            "confirm_password": "secret123",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    // Should now be logged in — can access protected route
    let launcher = app.server.get("/").await;
    let body = launcher.text();
    assert!(body.contains("LeanFin") || body.contains("Choose an application"));
}

#[tokio::test]
async fn invite_registration_password_mismatch_shows_error() {
    let app = harness::spawn_app().await;
    let token = myapps::auth::create_invite(&app.pool).await.unwrap();

    let response = app
        .server
        .post(&format!("/invite/{token}"))
        .form(&serde_json::json!({
            "username": "newuser",
            "password": "secret123",
            "confirm_password": "different",
        }))
        .await;
    let body = response.text();
    // Should re-render form with error
    assert!(body.contains(r#"name="username""#));
    assert!(body.contains("color:var(--danger)") || body.contains("mismatch"));
}

#[tokio::test]
async fn invite_token_cannot_be_reused() {
    let app = harness::spawn_app().await;
    let token = myapps::auth::create_invite(&app.pool).await.unwrap();

    // Use the invite
    app.server
        .post(&format!("/invite/{token}"))
        .form(&serde_json::json!({
            "username": "user1",
            "password": "pass",
            "confirm_password": "pass",
        }))
        .expect_failure()
        .await;

    // Try to access the invite page again
    let response = app.server.get(&format!("/invite/{token}")).await;
    let body = response.text();
    // Should show error, not the registration form
    assert!(!body.contains(r#"name="confirm_password""#));
}

#[tokio::test]
async fn invite_page_has_language_toggle() {
    let app = harness::spawn_app().await;
    let token = myapps::auth::create_invite(&app.pool).await.unwrap();

    let response = app.server.get(&format!("/invite/{token}")).await;
    let body = response.text();
    assert!(body.contains("Español") || body.contains("English"));
}
