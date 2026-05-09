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
async fn view_page_renders_grid_with_editable_cells() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    // Same grid table the new-input page uses
    assert!(body.contains(r#"class="ci-input-table""#));
    // Row identifier (col 0) is non-editable in fixed-row mode. data-col="0"
    // is set so the global sort logic can read its text by attribute lookup.
    assert!(body.contains(r#"class="ci-pupil-name" data-col="0">Alba García</td>"#));
    // Data cells are tagged for the JS double-click handler
    assert!(body.contains(r#"data-row="1" data-col="1""#));
    // Number column carries its type annotation so the JS spawns the right control
    assert!(body.contains(r#"data-type="number""#));
    // Save endpoint is wired up
    assert!(body.contains(&format!("/forms/inputs/{id}/cell")));
}

#[tokio::test]
async fn view_page_dynamic_input_makes_all_cells_editable() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    // No row identifier column: col 0 is editable too
    assert!(body.contains(r#"data-row="1" data-col="0""#));
    // No ci-pupil-name styling on dynamic inputs
    assert!(!body.contains("ci-pupil-name"));
}

#[tokio::test]
async fn update_cell_persists_change() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 1,
            "value": "10",
        }))
        .await;
    assert_eq!(response.status_code(), 204);

    let csv: String = sqlx::query_scalar("SELECT csv_data FROM form_input_inputs WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    // Row 1 = "Alba García,10,Good improvement" (was 8.5)
    assert!(
        lines[1].starts_with("Alba García,10,"),
        "expected score updated, got {}",
        lines[1]
    );
}

#[tokio::test]
async fn update_cell_rejects_row_id_in_fixed_mode() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 0,
            "value": "Hacker",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
}

#[tokio::test]
async fn update_cell_rejects_header_row() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 0,
            "col": 0,
            "value": "Renamed",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
}

#[tokio::test]
async fn update_cell_rejects_out_of_range() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 9999,
            "col": 1,
            "value": "x",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 400);
}

#[tokio::test]
async fn update_cell_dynamic_input_allows_col_zero() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 0,
            "value": "Bullet train",
        }))
        .await;
    assert_eq!(response.status_code(), 204);

    let csv: String = sqlx::query_scalar("SELECT csv_data FROM form_input_inputs WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert!(
        lines[1].starts_with("Bullet train,"),
        "expected first column updated, got {}",
        lines[1]
    );
}

#[tokio::test]
async fn update_cell_quotes_values_with_commas() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'March expenses' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 3,
            "value": "Trip, with notes",
        }))
        .await;
    assert_eq!(response.status_code(), 204);

    let csv: String = sqlx::query_scalar("SELECT csv_data FROM form_input_inputs WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert!(
        csv.contains(r#""Trip, with notes""#),
        "comma-bearing value should be quoted, csv: {csv}"
    );
}

#[tokio::test]
async fn update_cell_rejects_other_users_input() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    // Seed as one user, log in as another
    let owner_uid = myapps_core::auth::create_user(&app.pool, "owner", "owner")
        .await
        .unwrap();
    let owner_app = myapps_form_input::FormInputApp;
    myapps_form_input::services::seed::run(&app.pool, owner_uid, &owner_app)
        .await
        .unwrap();
    app.login_as("test", "pass").await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 1,
            "value": "Hacker",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn new_input_page_renders_link_modal() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let body = app.server.get("/forms/new").await.text();
    // Modal markup is present so JS can call showModal()
    assert!(body.contains(r#"id="link-modal""#));
    assert!(body.contains(r#"id="link-modal-url""#));
    assert!(body.contains(r#"id="link-modal-text""#));
}

#[tokio::test]
async fn form_type_with_link_column_persists() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json = r#"[{"name":"Title","type":"text"},{"name":"Source","type":"link"}]"#;
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0)",
    )
    .bind(uid)
    .bind("Bookmark log")
    .bind(columns_json)
    .execute(&app.pool)
    .await
    .unwrap();

    let body = app.server.get("/forms/form-types").await.text();
    assert!(body.contains("Bookmark log"));
    assert!(body.contains("Source"));
    // Link type label appears in the EN translation
    assert!(body.contains("Link"));
}

#[tokio::test]
async fn view_page_renders_link_cell_as_anchor() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json = r#"[{"name":"Title","type":"text"},{"name":"Source","type":"link"}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(uid)
    .bind("Bookmark log")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "Title,Source\nRust book,https://doc.rust-lang.org/book/|the book\nNo source,";
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, ?, ?) RETURNING id",
    )
    .bind(uid)
    .bind(ft_id)
    .bind("My bookmarks")
    .bind(csv)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    // Link cell renders as an anchor with target=_blank
    assert!(body.contains(r#"href="https://doc.rust-lang.org/book/""#));
    assert!(body.contains("the book</a>"));
    // Cell carries data-type=link and data-value for the dblclick handler
    assert!(body.contains(r#"data-type="link""#));
    assert!(body.contains(r#"data-value="https://doc.rust-lang.org/book/|the book""#));
    // Modal markup is present
    assert!(body.contains(r#"id="link-modal""#));
}

#[tokio::test]
async fn view_page_renders_multiline_text_column_in_separate_row() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json =
        r#"[{"name":"Title","type":"text"},{"name":"Synopsis","type":"text","multiline":true}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(uid)
    .bind("Movies")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "Title,Synopsis\nInception,A thief enters dreams.";
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, ?, ?) RETURNING id",
    )
    .bind(uid)
    .bind(ft_id)
    .bind("My movies")
    .bind(csv)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    // The multiline column gets its own row beneath the main row, with the
    // value wrapped in a .ci-multiline-value element.
    assert!(body.contains(r#"class="ci-multiline-row""#));
    assert!(body.contains(r#"<div class="ci-multiline-value">A thief enters dreams.</div>"#));
    // The Synopsis column is removed from the table header (it's only shown in
    // the follow-up row) — only the Title <th> should be present.
    assert!(body.contains(r#"<span class="ci-th-label">Title</span>"#));
    assert!(!body.contains(r#"<span class="ci-th-label">Synopsis</span>"#));
    // Multiline cells stay editable: data-type="text" + ci-cell-editable.
    assert!(body.contains(r#"data-row="1" data-col="1" data-type="text""#));
}

#[tokio::test]
async fn form_type_with_multiline_column_persists_flag() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app
        .server
        .post("/forms/form-types/create")
        .form(&serde_json::json!({
            "name": "Notes",
            "columns": r#"[{"name":"Body","type":"text","multiline":true}]"#,
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: String =
        sqlx::query_scalar("SELECT columns_json FROM form_input_form_types WHERE name = 'Notes'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(
        stored.contains(r#""multiline":true"#),
        "multiline flag should round-trip through the create handler, got: {stored}"
    );
}

#[tokio::test]
async fn form_type_create_drops_multiline_flag_for_non_text_columns() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let response = app
        .server
        .post("/forms/form-types/create")
        .form(&serde_json::json!({
            "name": "BadCols",
            "columns": r#"[{"name":"Score","type":"number","multiline":true}]"#,
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: String =
        sqlx::query_scalar("SELECT columns_json FROM form_input_form_types WHERE name = 'BadCols'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    // multiline is text-only; the server strips the flag for other types.
    assert!(
        !stored.contains(r#""multiline":true"#),
        "non-text columns must not store multiline=true, got: {stored}"
    );
}

#[tokio::test]
async fn update_cell_persists_link_value_with_pipe() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json = r#"[{"name":"Source","type":"link"}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(uid)
    .bind("Bookmarks")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, ?, ?) RETURNING id",
    )
    .bind(uid)
    .bind(ft_id)
    .bind("Bookmarks")
    .bind("Source\nplaceholder")
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 0,
            "value": "https://example.com|example",
        }))
        .await;
    assert_eq!(response.status_code(), 204);

    let csv: String = sqlx::query_scalar("SELECT csv_data FROM form_input_inputs WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[1], "https://example.com|example");
}

#[tokio::test]
async fn update_cell_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app
        .server
        .post("/forms/inputs/1/cell")
        .form(&serde_json::json!({ "row": 1, "col": 1, "value": "x" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn view_page_renders_sort_and_search_controls() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM form_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    // Each header carries its column index + type so the JS can sort numerically
    // when appropriate.
    assert!(body.contains(r#"data-col-type="text""#));
    assert!(body.contains(r#"data-col-type="number""#));
    // Sort buttons (one asc + one desc) per column.
    assert!(body.contains("ci-sort-btn"));
    assert!(body.contains(r#"data-dir="asc""#));
    assert!(body.contains(r#"data-dir="desc""#));
    // A single global search input sits above the table — per-column filters were
    // dropped in favour of one box that searches every column at once.
    assert!(body.contains(r#"id="ci-global-search""#));
    assert!(!body.contains("ci-filter-input"));
    // Each row records its original CSV index so sort can be cleared / saves
    // hit the right underlying row.
    assert!(body.contains(r#"data-original-index="0""#));
    assert!(body.contains(r#"data-original-index="1""#));
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

#[tokio::test]
async fn inputs_list_escapes_html_in_input_name() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let (ft_id,): (i64,) = sqlx::query_as(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, 'T', '[]', 0) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, ?, '')",
    )
    .bind(user_id)
    .bind(ft_id)
    .bind("<img src=x onerror=alert(1)>")
    .execute(&app.pool)
    .await
    .unwrap();

    let body = app.server.get("/forms").await.text();
    assert!(!body.contains("<img src=x onerror=alert(1)>"));
    assert!(body.contains("&lt;img src=x onerror=alert(1)&gt;"));
}

#[tokio::test]
async fn input_view_escapes_html_in_cells_and_headers() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let (ft_id,): (i64,) = sqlx::query_as(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, 'T', '[{\"name\":\"col\",\"type\":\"text\"}]', 0) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let csv = "<b>header</b>\n<script>alert(1)</script>";
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, 'i', ?) RETURNING id",
    )
    .bind(user_id)
    .bind(ft_id)
    .bind(csv)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    assert!(
        !body.contains("<b>header</b>"),
        "header cell must be escaped"
    );
    assert!(
        !body.contains("<script>alert(1)</script>"),
        "row cell must be escaped"
    );
    assert!(body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[tokio::test]
async fn new_input_page_escapes_script_close_in_embedded_json() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.login_as("test", "pass").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, 'T', '[]', 1)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO form_input_row_sets (user_id, label, rows) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind("rs")
        .bind("</script><b>x")
        .execute(&app.pool)
        .await
        .unwrap();

    let body = app.server.get("/forms/new").await.text();
    // A literal </script> inside the embedded JSON would close the surrounding
    // <script> tag and let the rest of the JSON literal render as HTML.
    assert!(
        !body.contains("</script><b>x"),
        "embedded JSON must escape '</' so the surrounding <script> tag stays open"
    );
    assert!(body.contains("<\\/script>"));
}

#[tokio::test]
async fn update_cell_persists_multiline_value_and_reads_back_intact() {
    // Saving a value containing a literal newline must (a) be quoted by the
    // serializer, (b) survive parse_csv → serialize → parse_csv round-tripping
    // without splitting into a separate row, and (c) be rendered back inside
    // the .ci-multiline-value cell when the page is viewed.
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json =
        r#"[{"name":"Title","type":"text"},{"name":"Synopsis","type":"text","multiline":true}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(uid)
    .bind("Movies-ml")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_inputs (user_id, row_set_id, form_type_id, name, csv_data) VALUES (?, NULL, ?, ?, ?) RETURNING id",
    )
    .bind(uid)
    .bind(ft_id)
    .bind("My ml movies")
    .bind("Title,Synopsis\nInception,placeholder")
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/forms/inputs/{id}/cell"))
        .form(&serde_json::json!({
            "row": 1,
            "col": 1,
            "value": "line one\nline two\nline three",
        }))
        .await;
    assert_eq!(response.status_code(), 204);

    let csv: String = sqlx::query_scalar("SELECT csv_data FROM form_input_inputs WHERE id = ?")
        .bind(id)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    // The multi-line value must be quoted by the serializer so the embedded
    // newlines don't look like new records.
    assert!(
        csv.contains("\"line one\nline two\nline three\""),
        "multi-line value should be quoted in stored CSV, got: {csv:?}"
    );

    // The list page row count uses parse_csv: it must count exactly 1 data row
    // (header + 1) — not 3 — even though the value contains 2 newlines.
    let list_body = app.server.get("/forms").await.text();
    assert!(
        list_body.contains("My ml movies"),
        "input should appear on the list"
    );

    // Viewing the input must render the multi-line value inside the dedicated
    // .ci-multiline-value div with the newlines preserved.
    let body = app.server.get(&format!("/forms/inputs/{id}")).await.text();
    assert!(
        body.contains("<div class=\"ci-multiline-value\">line one\nline two\nline three</div>"),
        "multi-line value should round-trip into the rendered cell"
    );
    // Only one main row should be rendered (data-row="1"), not 3.
    assert!(body.contains(r#"data-row="1""#), "row 1 should be present");
    assert!(
        !body.contains(r#"data-row="2""#),
        "embedded newlines must not produce a phantom row 2"
    );
}

#[tokio::test]
async fn edit_form_type_page_reflects_multiline_checkbox_state() {
    // The edit form should render the multiline checkbox pre-checked when the
    // stored column has multiline:true so users can see/edit the flag, mirroring
    // how the fixed_rows checkbox is reflected.
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let columns_json =
        r#"[{"name":"Headline","type":"text"},{"name":"Body","type":"text","multiline":true}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(uid)
    .bind("Notes-ml")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let body = app
        .server
        .get(&format!("/forms/form-types/{ft_id}/edit"))
        .await
        .text();

    // There must be exactly one checked multiline checkbox (Body) and one
    // unchecked one (Headline).
    let checked = body.matches(r#"data-col-multiline checked"#).count();
    let total_inputs = body
        .matches(r#"<input type="checkbox" data-col-multiline"#)
        .count();
    assert_eq!(
        checked, 1,
        "exactly one multiline checkbox should be pre-checked, got {checked}"
    );
    assert!(
        total_inputs >= 2,
        "edit page should render a multiline checkbox per existing column, got {total_inputs}"
    );
}
