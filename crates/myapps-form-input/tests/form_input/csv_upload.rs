use axum_test::multipart::{MultipartForm, Part};

fn csv_part(csv: &str) -> Part {
    Part::bytes(csv.as_bytes().to_vec())
        .file_name("upload.csv")
        .mime_type("text/csv")
}

async fn dynamic_form_type(app: &myapps_test_harness::TestApp) -> i64 {
    sqlx::query_scalar("SELECT id FROM form_input_form_types WHERE name = 'Expense log' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

/// Create a tiny 2-row fixed-row scenario tailored for CSV-upload tests so the
/// assertions stay readable. Returns (row_set_id, form_type_id).
async fn make_fixed_scenario(app: &myapps_test_harness::TestApp) -> (i64, i64) {
    let (uid,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'seeduser' LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let rs_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_row_sets (user_id, label, rows) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(uid)
    .bind("CSV-tiny")
    .bind("Alpha\nBravo")
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let columns_json = r#"[{"name":"Score","type":"number"},{"name":"Comment","type":"text"}]"#;
    let ft_id: i64 = sqlx::query_scalar(
        "INSERT INTO form_input_form_types (user_id, name, columns_json, fixed_rows) VALUES (?, ?, ?, 1) RETURNING id",
    )
    .bind(uid)
    .bind("CSV-tiny-quiz")
    .bind(columns_json)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    (rs_id, ft_id)
}

#[tokio::test]
async fn new_input_page_renders_csv_upload_tab() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;

    let body = app.server.get("/forms/new").await.text();
    assert!(
        body.contains(r#"id="tab-btn-csv""#),
        "missing CSV tab button"
    );
    assert!(
        body.contains(r#"action="/forms/inputs/create-from-csv""#),
        "missing CSV form action"
    );
    assert!(
        body.contains(r#"enctype="multipart/form-data""#),
        "missing multipart enctype"
    );
    assert!(
        body.contains(r#"name="file""#),
        "missing file input on CSV form"
    );
}

#[tokio::test]
async fn csv_upload_creates_dynamic_input() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let ft_id = dynamic_form_type(&app).await;

    let csv = "Train,12.50,Yes,Commute\nLunch,8.20,No,Team meeting";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Expenses via CSV")
                .add_text("form_type_id", ft_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: (String, Option<i64>) = sqlx::query_as(
        "SELECT csv_data, row_set_id FROM form_input_inputs WHERE name = 'Expenses via CSV' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(
        stored.1.is_none(),
        "dynamic input should have NULL row_set_id"
    );
    let lines: Vec<&str> = stored.0.lines().collect();
    assert_eq!(lines.len(), 3, "expected header + 2 rows");
    assert_eq!(lines[0], "Item,Amount,Reimbursable,Notes");
    assert_eq!(lines[1], "Train,12.50,Yes,Commute");
    assert_eq!(lines[2], "Lunch,8.20,No,Team meeting");
}

#[tokio::test]
async fn csv_upload_strips_optional_header() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let ft_id = dynamic_form_type(&app).await;

    // Header line with the same column names — should be detected and dropped.
    let csv = "Item,Amount,Reimbursable,Notes\nTaxi,15,No,Late night";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "With header")
                .add_text("form_type_id", ft_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: String = sqlx::query_scalar(
        "SELECT csv_data FROM form_input_inputs WHERE name = 'With header' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    let lines: Vec<&str> = stored.lines().collect();
    assert_eq!(lines.len(), 2, "user-supplied header should be dropped");
    assert_eq!(lines[0], "Item,Amount,Reimbursable,Notes");
    assert_eq!(lines[1], "Taxi,15,No,Late night");
}

#[tokio::test]
async fn csv_upload_creates_fixed_rows_input() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let (rs_id, ft_id) = make_fixed_scenario(&app).await;

    let csv = "Alpha,9,Excellent\nBravo,7,Good";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Quiz CSV")
                .add_text("form_type_id", ft_id.to_string())
                .add_text("row_set_id", rs_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: (String, Option<i64>) = sqlx::query_as(
        "SELECT csv_data, row_set_id FROM form_input_inputs WHERE name = 'Quiz CSV' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(stored.1, Some(rs_id), "row_set_id must be persisted");
    let lines: Vec<&str> = stored.0.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "Row,Score,Comment");
    assert_eq!(lines[1], "Alpha,9,Excellent");
    assert_eq!(lines[2], "Bravo,7,Good");
}

#[tokio::test]
async fn csv_upload_rejects_wrong_column_count() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let ft_id = dynamic_form_type(&app).await;

    // Expense log expects 4 columns; this CSV only has 2.
    let csv = "Train,12.50";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Bad columns")
                .add_text("form_type_id", ft_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .await;
    let body = response.text();
    assert!(
        body.contains("Import failed") && body.contains("expected 4 columns"),
        "expected column-count error page, body: {body}"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM form_input_inputs WHERE name = 'Bad columns'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "no input should be persisted on validation error");
}

#[tokio::test]
async fn csv_upload_rejects_row_count_mismatch_in_fixed_mode() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let (rs_id, ft_id) = make_fixed_scenario(&app).await;

    // Row set has 2 entries; CSV provides 3.
    let csv = "Alpha,9,a\nBravo,7,b\nCharlie,5,c";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Bad row count")
                .add_text("form_type_id", ft_id.to_string())
                .add_text("row_set_id", rs_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .await;
    let body = response.text();
    assert!(
        body.contains("Row count mismatch"),
        "expected row-count error, body: {body}"
    );
}

#[tokio::test]
async fn csv_upload_rejects_key_mismatch_in_fixed_mode() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let (rs_id, ft_id) = make_fixed_scenario(&app).await;

    // First-column keys must equal the row-set entries in order.
    let csv = "Alpha,9,a\nDelta,7,b";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Bad key")
                .add_text("form_type_id", ft_id.to_string())
                .add_text("row_set_id", rs_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .await;
    let body = response.text();
    assert!(
        body.contains("does not match row-set entry"),
        "expected key mismatch error, body: {body}"
    );
}

#[tokio::test]
async fn csv_upload_rejects_missing_file() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let ft_id = dynamic_form_type(&app).await;

    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "No file")
                .add_text("form_type_id", ft_id.to_string()),
        )
        .await;
    let body = response.text();
    assert!(body.contains("No CSV file uploaded"), "body: {body}");
}

#[tokio::test]
async fn csv_upload_preserves_quoted_multiline_field() {
    // build_csv_from_upload now uses parse_csv, so a quoted multi-line cell in
    // the uploaded CSV must round-trip into storage as a single row, with the
    // embedded newline preserved (and the value re-quoted by the serializer).
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    app.seed_and_login(&myapps_form_input::FormInputApp).await;
    let ft_id = dynamic_form_type(&app).await;

    // 4 columns to match Expense log; the Notes field spans two lines.
    let csv = "Train,12.50,Yes,\"first line\nsecond line\"\nLunch,8.20,No,Quick";
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "Multi-line CSV")
                .add_text("form_type_id", ft_id.to_string())
                .add_part("file", csv_part(csv)),
        )
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let stored: String = sqlx::query_scalar(
        "SELECT csv_data FROM form_input_inputs WHERE name = 'Multi-line CSV' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    // Multi-line value must be quoted on the way out so re-parsing recovers it.
    assert!(
        stored.contains("\"first line\nsecond line\""),
        "multi-line cell should be quoted in stored CSV, got: {stored:?}"
    );
    // The lines() count alone would be misleading — count rows via parse-style:
    // header + 2 data rows means 3 records, but raw newlines = 4. Use a rough
    // check on the actual record content instead.
    assert!(
        stored.contains("Lunch,8.20,No,Quick"),
        "second record should be intact, got: {stored:?}"
    );
    // The embedded newline must not have produced a phantom 3-column row.
    assert!(
        !stored.contains("\nsecond line,"),
        "embedded newline must not be treated as a record separator, got: {stored:?}"
    );
}

#[tokio::test]
async fn csv_upload_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_form_input::FormInputApp)]).await;
    let response = app
        .server
        .post("/forms/inputs/create-from-csv")
        .multipart(
            MultipartForm::new()
                .add_text("name", "x")
                .add_text("form_type_id", "1")
                .add_part("file", csv_part("a,b,c\n1,2,3")),
        )
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}
