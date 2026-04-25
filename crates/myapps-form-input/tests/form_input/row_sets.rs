#[tokio::test]
async fn row_sets_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app.server.get("/forms/row-sets").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn row_sets_page_renders_seeded_row_sets() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms/row-sets").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    assert!(body.contains("2-A"));
}

#[tokio::test]
async fn row_sets_page_shows_row_count() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms/row-sets").await;
    let body = response.text();
    // 1-A has 15 rows
    assert!(body.contains("15 rows"));
    // 1-B has 14 rows
    assert!(body.contains("14 rows"));
}

#[tokio::test]
async fn row_sets_page_has_create_form() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/forms/row-sets").await;
    let body = response.text();
    assert!(body.contains(r#"name="label""#));
    assert!(body.contains(r#"name="rows""#));
}

#[tokio::test]
async fn create_row_set() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/forms/row-sets/create")
        .form(&serde_json::json!({
            "label": "3-C",
            "rows": "Ana García\nPedro López\nMaría Torres",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/forms/row-sets").await;
    let body = list.text();
    assert!(body.contains("3-C"));
    assert!(body.contains("3 rows"));
}

#[tokio::test]
async fn delete_row_set() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_row_sets WHERE label = '2-A' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/row-sets/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/forms/row-sets").await;
    let body = list.text();
    assert!(!body.contains("2-A"));
}
