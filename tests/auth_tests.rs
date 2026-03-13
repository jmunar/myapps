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
