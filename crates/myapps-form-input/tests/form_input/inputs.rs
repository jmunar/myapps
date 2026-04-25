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

#[tokio::test]
async fn dynamic_input_seeded_and_listed_without_row_set() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id, row_set_id): (i64, Option<i64>) = sqlx::query_as(
        "SELECT id, row_set_id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        row_set_id.is_none(),
        "dynamic input should have NULL row_set_id"
    );

    let body = app.server.get("/forms").await.text();
    assert!(body.contains("March expenses"));
    assert!(body.contains("Expense log"));

    let detail = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    assert!(detail.contains("Train ticket"));
    assert!(detail.contains("Office supplies"));
}

#[tokio::test]
async fn create_dynamic_input_ignores_row_set_id() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (rs_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_row_sets WHERE label = '1-A' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let (ft_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Expense log' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post("/forms/inputs/create")
        .form(&serde_json::json!({
            "row_set_id": rs_id,
            "form_type_id": ft_id,
            "name": "April expenses",
            "csv_data": "Item,Amount,Reimbursable,Notes\nFlight,420,Yes,Trip",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: Option<i64> = sqlx::query_scalar(
        "SELECT row_set_id FROM form_input_inputs WHERE name = 'April expenses' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        stored.is_none(),
        "row_set_id should be cleared for a dynamic form type even when posted"
    );
}

#[tokio::test]
async fn new_input_page_hides_row_set_warning_when_dynamic_form_type_exists() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let body = app.server.get("/forms/new").await.text();
    // The page tags each form type with a fixed_rows boolean for the JS toggle
    assert!(body.contains(r#""fixed_rows":true"#));
    assert!(body.contains(r#""fixed_rows":false"#));
    // The row-set group is present but JS hides it for dynamic mode
    assert!(body.contains(r#"id="row-set-group""#));
    assert!(body.contains(r#"id="add-row-btn""#));
}
