#[tokio::test]
async fn inputs_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app.server.get("/forms").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn inputs_page_renders_seeded_inputs() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Week 10 quiz"));
    assert!(body.contains("Week 11 quiz"));
    assert!(body.contains("Attendance"));
    assert!(body.contains("Reading assessment"));
}

#[tokio::test]
async fn inputs_page_shows_row_set_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms").await;
    let body = response.text();
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    assert!(body.contains("2-A"));
}

#[tokio::test]
async fn inputs_page_has_new_input_button() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms").await;
    let body = response.text();
    assert!(body.contains("/forms/new"));
}

#[tokio::test]
async fn new_input_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app.server.get("/forms/new").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn new_input_page_renders_with_dropdowns() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms/new").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("New Input"));
    assert!(body.contains(r#"name="row_set_id""#));
    assert!(body.contains(r#"name="form_type_id""#));
    assert!(body.contains(r#"name="name""#));
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Attendance"));
}

#[tokio::test]
async fn view_input_detail() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app.server.get(&format!("/forms/inputs/{id}")).await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Week 10 quiz"));
    assert!(body.contains("Alba"));
    assert!(body.contains("Carlos"));
}

#[tokio::test]
async fn create_input() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (rs_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_row_sets WHERE label = '1-A' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (ft_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Weekly quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post("/forms/inputs/create")
        .form(&serde_json::json!({
            "row_set_id": rs_id,
            "form_type_id": ft_id,
            "name": "Week 12 quiz",
            "csv_data": "Row,Score,Comment\nAlba García,9,Great\nCarlos López,8,Good",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/forms").await;
    let body = list.text();
    assert!(body.contains("Week 12 quiz"));
}

#[tokio::test]
async fn delete_input() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/forms").await;
    let body = list.text();
    assert!(!body.contains("Week 10 quiz"));
}
