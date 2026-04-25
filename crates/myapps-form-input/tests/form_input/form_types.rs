#[tokio::test]
async fn form_types_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app.server.get("/forms/form-types").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn form_types_page_renders_seeded_types() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms/form-types").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Attendance"));
    assert!(body.contains("Reading assessment"));
    assert!(body.contains("Behaviour report"));
}

#[tokio::test]
async fn form_types_page_shows_column_info() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app.server.get("/forms/form-types").await;
    let body = response.text();
    assert!(body.contains("Score"));
    assert!(body.contains("number"));
    assert!(body.contains("Present"));
    assert!(body.contains("bool"));
}

#[tokio::test]
async fn form_types_page_has_create_form() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/forms/form-types").await;
    let body = response.text();
    assert!(body.contains(r#"name="name""#));
    // Column rows are tagged with data-* attributes; the submit handler
    // serializes them into a single hidden `columns` field as JSON.
    assert!(body.contains("data-col-name"));
    assert!(body.contains("data-col-type"));
    assert!(body.contains(r#"name="columns""#));
}

#[tokio::test]
async fn create_form_type_via_post_persists_columns_json() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/forms/form-types/create")
        .form(&serde_json::json!({
            "name": "Bookmark log",
            "columns": r#"[{"name":"Title","type":"text"},{"name":"MyLink","type":"link"}]"#,
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let columns_json: String = sqlx::query_scalar(
        "SELECT columns_json FROM form_input_form_types WHERE name = 'Bookmark log' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(columns_json.contains(r#""name":"Title""#));
    assert!(columns_json.contains(r#""name":"MyLink""#));
    assert!(columns_json.contains(r#""type":"link""#));
}

#[tokio::test]
async fn create_form_type_via_post_with_single_link_column() {
    // Regression: a single-column form posted as JSON used to fail with
    // serde_urlencoded's "expected a sequence" error when col_name[]/col_type[]
    // were repeated keys.
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/forms/form-types/create")
        .form(&serde_json::json!({
            "name": "Single link form",
            "columns": r#"[{"name":"MyLink","type":"link"}]"#,
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let columns_json: String = sqlx::query_scalar(
        "SELECT columns_json FROM form_input_form_types WHERE name = 'Single link form' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(columns_json, r#"[{"name":"MyLink","type":"link"}]"#);
}

#[tokio::test]
async fn edit_form_type_via_post_replaces_columns() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Weekly quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/form-types/{id}/edit"))
        .form(&serde_json::json!({
            "name": "Weekly quiz",
            "columns": r#"[{"name":"Score","type":"number"},{"name":"Resources","type":"link"}]"#,
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let columns_json: String =
        sqlx::query_scalar("SELECT columns_json FROM form_input_form_types WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(columns_json.contains(r#""name":"Resources""#));
    assert!(columns_json.contains(r#""type":"link""#));
}

#[tokio::test]
async fn create_form_type() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let columns_json = r#"[{"name":"Grade","type":"number"},{"name":"Notes","type":"text"}]"#;
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json) VALUES (?, 'Homework', ?)",
    )
    .bind(user_id)
    .bind(columns_json)
    .execute(&app.pool)
    .await
    .unwrap();

    let list = app.server.get("/forms/form-types").await;
    let body = list.text();
    assert!(body.contains("Homework"));
    assert!(body.contains("Grade"));
}

#[tokio::test]
async fn edit_form_type_page_renders() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Weekly quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/forms/form-types/{id}/edit"))
        .await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Score"));
}

#[tokio::test]
async fn edit_form_type() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Weekly quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let columns_json = r#"[{"name":"Score","type":"number"},{"name":"Feedback","type":"text"}]"#;
    sqlx::query(
        "UPDATE form_input_form_types SET name = 'Monthly quiz', columns_json = ? WHERE id = ?",
    )
    .bind(columns_json)
    .bind(id)
    .execute(&app.pool)
    .await
    .unwrap();

    let list = app.server.get("/forms/form-types").await;
    let body = list.text();
    assert!(body.contains("Monthly quiz"));
    assert!(body.contains("Feedback"));
}

#[tokio::test]
async fn form_types_list_shows_fixed_and_dynamic_badges() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let body = app.server.get("/forms/form-types").await.text();
    assert!(body.contains("Fixed rows"));
    assert!(body.contains("Dynamic rows"));
}

#[tokio::test]
async fn create_form_type_with_fixed_rows_persists_flag() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, 'Roll call', '[]', 1)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, 'Free notes', '[]', 0)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let stored: (bool, bool) = sqlx::query_as(
        "SELECT (SELECT fixed_rows FROM form_input_form_types WHERE name = 'Roll call'),
                (SELECT fixed_rows FROM form_input_form_types WHERE name = 'Free notes')",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(stored.0, "Roll call should be fixed_rows=true");
    assert!(!stored.1, "Free notes should be fixed_rows=false");
}

#[tokio::test]
async fn fixed_rows_column_defaults_to_false() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    // Insert without specifying fixed_rows — the migration's DEFAULT 0 applies.
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json) VALUES (?, 'Quick log', '[]')",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let stored: bool = sqlx::query_scalar(
        "SELECT fixed_rows FROM form_input_form_types WHERE name = 'Quick log' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(!stored, "default fixed_rows should be false");
}

#[tokio::test]
async fn edit_form_type_page_reflects_fixed_rows_checkbox_state() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (fixed_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Weekly quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    let (dyn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_form_types WHERE name = 'Expense log' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let fixed_body = app
        .server
        .get(&format!("/forms/form-types/{fixed_id}/edit"))
        .await
        .text();
    assert!(
        fixed_body.contains(r#"name="fixed_rows" value="1" checked"#),
        "fixed_rows checkbox should be checked for fixed-rows form type"
    );

    let dyn_body = app
        .server
        .get(&format!("/forms/form-types/{dyn_id}/edit"))
        .await
        .text();
    assert!(
        dyn_body.contains(r#"name="fixed_rows" value="1">"#)
            && !dyn_body.contains(r#"name="fixed_rows" value="1" checked"#),
        "fixed_rows checkbox should not be checked for dynamic form type"
    );
}

#[tokio::test]
async fn delete_form_type() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM form_input_form_types WHERE name = 'Behaviour report' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/forms/form-types/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/forms/form-types").await;
    let body = list.text();
    assert!(!body.contains("Behaviour report"));
}
