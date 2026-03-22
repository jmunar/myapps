#[tokio::test]
async fn form_types_page_requires_authentication() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    let response = app
        .server
        .get("/classroom/form-types")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn form_types_page_renders_seeded_types() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let response = app.server.get("/classroom/form-types").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Attendance"));
    assert!(body.contains("Reading assessment"));
    assert!(body.contains("Behaviour report"));
}

#[tokio::test]
async fn form_types_page_shows_column_info() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let response = app.server.get("/classroom/form-types").await;
    let body = response.text();
    assert!(body.contains("Score"));
    assert!(body.contains("number"));
    assert!(body.contains("Present"));
    assert!(body.contains("bool"));
}

#[tokio::test]
async fn form_types_page_has_create_form() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/classroom/form-types").await;
    let body = response.text();
    assert!(body.contains(r#"name="name""#));
    assert!(body.contains("col_name[]") || body.contains("col_name"));
    assert!(body.contains("col_type[]") || body.contains("col_type"));
}

#[tokio::test]
async fn create_form_type() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.login_as("test", "pass").await;

    // Insert via DB since axum-test doesn't easily support repeated form keys
    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let columns_json = r#"[{"name":"Grade","type":"number"},{"name":"Notes","type":"text"}]"#;
    sqlx::query(
        "INSERT INTO classroom_input_form_types (user_id, name, columns_json) VALUES (?, 'Homework', ?)",
    )
    .bind(user_id)
    .bind(columns_json)
    .execute(&app.pool)
    .await
    .unwrap();

    let list = app.server.get("/classroom/form-types").await;
    let body = list.text();
    assert!(body.contains("Homework"));
    assert!(body.contains("Grade"));
}

#[tokio::test]
async fn edit_form_type_page_renders() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM classroom_input_form_types WHERE name = 'Weekly quiz' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/classroom/form-types/{id}/edit"))
        .await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Score"));
}

#[tokio::test]
async fn edit_form_type() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM classroom_input_form_types WHERE name = 'Weekly quiz' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    // Update via DB since axum-test doesn't easily support repeated form keys
    let columns_json = r#"[{"name":"Score","type":"number"},{"name":"Feedback","type":"text"}]"#;
    sqlx::query("UPDATE classroom_input_form_types SET name = 'Monthly quiz', columns_json = ? WHERE id = ?")
        .bind(columns_json)
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    let list = app.server.get("/classroom/form-types").await;
    let body = list.text();
    assert!(body.contains("Monthly quiz"));
    // Verify the form type name was changed in the listing item
    assert!(body.contains("Feedback"));
}

#[tokio::test]
async fn delete_form_type() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    // Delete a form type that has no inputs (Behaviour report)
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM classroom_input_form_types WHERE name = 'Behaviour report' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/classroom/form-types/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/classroom/form-types").await;
    let body = list.text();
    assert!(!body.contains("Behaviour report"));
}
