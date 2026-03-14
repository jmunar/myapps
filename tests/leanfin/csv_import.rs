use crate::harness;

#[tokio::test]
async fn import_csv_form_renders_for_manual_account_owner() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .await;
    let body = response.text();
    assert!(body.contains("Import CSV"), "missing Import CSV title");
    assert!(
        body.contains("multipart/form-data"),
        "missing multipart enctype"
    );
    assert!(
        body.contains(r#"name="file""#),
        "missing file input"
    );
    assert!(
        body.contains("YYYY-MM-DD"),
        "missing format instructions"
    );
}

#[tokio::test]
async fn import_csv_button_appears_on_accounts_list() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/accounts").await;
    let body = response.text();
    assert!(body.contains("Import CSV"), "missing Import CSV button");
}

#[tokio::test]
async fn successful_import_updates_balances() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "date,value\n2027-06-01,20000.00\n2027-07-01,21000.50\n2027-08-01,22500.00\n";

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_part(
                    "file",
                    axum_test::multipart::Part::bytes(csv.as_bytes().to_vec())
                        .file_name("data.csv")
,
                ),
        )
        .await;
    let body = response.text();
    assert!(body.contains("3 row(s) imported"), "missing import count: {body}");
    assert!(body.contains("Import Complete"), "missing success page");

    // Verify balance_snapshots rows
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM balance_snapshots WHERE account_id = ? AND date IN ('2027-06-01','2027-07-01','2027-08-01')",
    )
    .bind(id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 3, "expected 3 balance_snapshots rows");

    // Verify account balance updated to latest
    let (balance,): (f64,) =
        sqlx::query_as("SELECT balance_amount FROM accounts WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert!(
        (balance - 22500.0).abs() < 0.01,
        "balance_amount not updated to latest: {balance}"
    );
}

#[tokio::test]
async fn invalid_rows_reject_entire_import() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "date,value\n2025-06-01,20000.00\nbad-date,21000.50\n2025-08-01,not-a-number\n";

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(
            axum_test::multipart::MultipartForm::new().add_part(
                "file",
                axum_test::multipart::Part::bytes(csv.as_bytes().to_vec())
                    .file_name("data.csv")
,
            ),
        )
        .await;
    let body = response.text();
    assert!(body.contains("Import Failed"), "should show failure page");
    assert!(body.contains("Line 3"), "should mention line 3");
    assert!(body.contains("Line 4"), "should mention line 4");

    // No rows should have been written
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM balance_snapshots WHERE account_id = ? AND date = '2025-06-01'",
    )
    .bind(id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "no rows should be written on error");
}

#[tokio::test]
async fn missing_columns_rejected() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "foo,bar\n2025-06-01,20000.00\n";

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(
            axum_test::multipart::MultipartForm::new().add_part(
                "file",
                axum_test::multipart::Part::bytes(csv.as_bytes().to_vec())
                    .file_name("data.csv")
,
            ),
        )
        .await;
    let body = response.text();
    assert!(body.contains("Import Failed"), "should show failure page");
    assert!(
        body.contains("Missing required column"),
        "should mention missing column: {body}"
    );
}

#[tokio::test]
async fn empty_file_rejected() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(
            axum_test::multipart::MultipartForm::new().add_part(
                "file",
                axum_test::multipart::Part::bytes(Vec::new())
                    .file_name("empty.csv")
,
            ),
        )
        .await;
    let body = response.text();
    assert!(body.contains("Import Failed"), "should show failure page");
}

#[tokio::test]
async fn duplicate_import_is_idempotent() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "date,value\n2025-06-01,20000.00\n2025-07-01,21000.50\n";

    let form = || {
        axum_test::multipart::MultipartForm::new().add_part(
            "file",
            axum_test::multipart::Part::bytes(csv.as_bytes().to_vec())
                .file_name("data.csv"),
        )
    };

    // Import twice
    app.server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(form())
        .await;

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(form())
        .await;
    let body = response.text();
    assert!(body.contains("2 row(s) imported"), "second import should also succeed");

    // Should still only have 2 rows (not 4)
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM balance_snapshots WHERE account_id = ? AND date IN ('2025-06-01','2025-07-01')",
    )
    .bind(id)
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(count, 2, "should have exactly 2 rows after duplicate import");
}

#[tokio::test]
async fn non_owner_gets_redirected() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    // Get the manual account ID (owned by demo user)
    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    // Create and log in as a different user
    let app2 = harness::spawn_app().await;
    app2.login_as("other", "other").await;

    // Try to access the form — should redirect
    let response = app2
        .server
        .get(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn balance_alias_column_accepted() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let (id,): (i64,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE account_type = 'manual' AND account_name = 'Stock Portfolio'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let csv = "date,balance\n2025-09-01,30000.00\n";

    let response = app
        .server
        .post(&format!("/leanfin/accounts/manual/{id}/import-csv"))
        .multipart(
            axum_test::multipart::MultipartForm::new().add_part(
                "file",
                axum_test::multipart::Part::bytes(csv.as_bytes().to_vec())
                    .file_name("data.csv")
,
            ),
        )
        .await;
    let body = response.text();
    assert!(body.contains("1 row(s) imported"), "balance alias not accepted");
}
