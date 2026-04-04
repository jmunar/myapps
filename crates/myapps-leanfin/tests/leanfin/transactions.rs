// ── Pagination ──────────────────────────────────────────────

#[tokio::test]
async fn transaction_list_shows_pagination_controls() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();

    // Pagination info shows "1–N of M"
    assert!(body.contains(r#"class="pagination-info""#));
    assert!(body.contains("of"));
    // Pagination container exists
    assert!(body.contains(r#"class="pagination""#));
}

#[tokio::test]
async fn transaction_list_page_param_defaults_to_first_page() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // No page param and page=1 should return the same results
    let response_default = app.server.get("/leanfin/transactions").await;
    let response_page1 = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("page", "1")
        .await;

    assert_eq!(response_default.text(), response_page1.text());
}

#[tokio::test]
async fn transaction_list_pagination_with_many_transactions() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Get an existing seeded account
    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Delete existing transactions and insert exactly 60 (page size is 50)
    sqlx::query("DELETE FROM leanfin_allocations")
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM leanfin_transactions")
        .execute(&app.pool)
        .await
        .unwrap();

    for i in 0..60 {
        sqlx::query(
            "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, counterparty) VALUES (?, ?, ?, ?, 'EUR', ?, ?)"
        )
        .bind(account_id)
        .bind(format!("ext-{i}"))
        .bind(format!("2025-01-{:02}", (i % 28) + 1))
        .bind(-10.0)
        .bind(format!("Txn {i}"))
        .bind(format!("Vendor {i}"))
        .execute(&app.pool)
        .await
        .unwrap();
    }

    // Page 1 should have Next button but no Prev
    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("1\u{2013}50 of 60"));
    assert!(body.contains(">Next</button>"));
    assert!(!body.contains(">Prev</button>"));

    // Page 2 should have Prev button but no Next
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("page", "2")
        .await;
    let body = response.text();
    assert!(body.contains("51\u{2013}60 of 60"));
    assert!(body.contains(">Prev</button>"));
    assert!(!body.contains(">Next</button>"));
}

#[tokio::test]
async fn transaction_pagination_preserves_filters_in_buttons() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();

    // Pagination buttons should include hx-include for filters
    if body.contains(">Next</button>") || body.contains(">Prev</button>") {
        assert!(body.contains(r##"hx-include="#txn-filters""##));
    }
}

// ── Dashboard label filter ───────────────────────────────────

#[tokio::test]
async fn dashboard_has_label_ids_filter() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    // label_ids select filter exists in the dashboard
    assert!(body.contains(r#"name="label_ids""#));
    assert!(body.contains("All labels"));
    assert!(body.contains("Groceries"));
}

#[tokio::test]
async fn dashboard_nav_includes_expenses_tab() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    assert!(body.contains("/leanfin/expenses"));
    assert!(body.contains("Expenses"));
}

// ── Transaction label_ids filter ─────────────────────────────

#[tokio::test]
async fn transaction_label_ids_filter_returns_matching() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("label_ids", &label_id.to_string())
        .await;
    let body = response.text();

    // Groceries-labeled transactions include Mercadona
    assert!(body.contains("Mercadona"));
    // Should not contain transactions only labeled differently
    assert!(!body.contains("Netflix"));
}

#[tokio::test]
async fn transaction_label_ids_filter_empty_returns_all() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Empty label_ids param should not filter (returns all transactions)
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("label_ids", "")
        .await;
    let body = response.text();

    assert!(body.contains("Mercadona"));
    assert!(body.contains("Netflix"));
}

// ── Transaction date_from / date_to filters ──────────────────

#[tokio::test]
async fn transaction_date_from_filter() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Use a far-future date so nothing matches
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_from", "2099-01-01")
        .await;
    let body = response.text();

    // No transactions in the future → empty state
    assert!(body.contains("No transactions yet"));
}

#[tokio::test]
async fn transaction_date_to_filter() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Use a far-past date so nothing matches
    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_to", "1900-01-01")
        .await;
    let body = response.text();

    assert!(body.contains("No transactions yet"));
}

#[tokio::test]
async fn transaction_date_range_filter_returns_subset() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Get a date that exists in seed data
    let (some_date,): (String,) =
        sqlx::query_as("SELECT date FROM leanfin_transactions ORDER BY date DESC LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("date_from", &some_date)
        .add_query_param("date_to", &some_date)
        .await;
    let body = response.text();

    // Should return at least the transaction(s) on that date
    assert!(response.status_code().is_success());
    assert!(body.contains(&some_date));
}

#[tokio::test]
async fn dashboard_loads_with_htmx_container() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();
    assert!(body.contains("Transactions"));
    assert!(body.contains("hx-get"));
}

#[tokio::test]
async fn transaction_list_returns_table() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("Mercadona"));
    assert!(body.contains("Netflix"));
}

#[tokio::test]
async fn transaction_search_filters_results() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("q", "Netflix")
        .await;
    let body = response.text();
    assert!(body.contains("Netflix"));
    assert!(!body.contains("Mercadona"));
}

#[tokio::test]
async fn transaction_unallocated_filter() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("unallocated", "1")
        .await;
    let body = response.text();
    // The seed data leaves some transactions unallocated (the else-continue branch)
    // We just verify it returns a valid response
    assert!(response.status_code().is_success());
    assert!(!body.is_empty());
}

#[tokio::test]
async fn transaction_account_filter() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Get a specific account id from the DB
    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'ING Direct'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/transactions")
        .add_query_param("account_id", &account_id.to_string())
        .await;
    let body = response.text();
    // ING Direct savings account has "MyInvestor" and "Self transfer" transactions
    assert!(body.contains("Self transfer") || body.contains("MyInvestor"));
    // Should not contain checking-only transactions
    assert!(!body.contains("Mercadona"));
}

#[tokio::test]
async fn transaction_list_has_balance_column() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("<th>Balance</th>"));
}

#[tokio::test]
async fn transaction_balance_shows_value_when_present() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Pick a transaction and set its balance_after
    let (txn_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_transactions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query("UPDATE leanfin_transactions SET balance_after = ? WHERE id = ?")
        .bind(1500.50_f64)
        .bind(txn_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("1500.50"));
    assert!(body.contains(r#"class="txn-balance""#));
}

#[tokio::test]
async fn transaction_balance_shows_dash_when_null() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    // Seed data has no balance_after set, so all balances should show "—"
    assert!(body.contains(r#"class="txn-balance">"#));
    assert!(body.contains(">\u{2014}</td>"));
}

// ── Allocation editor: Add Rule form ────────────────────────

#[tokio::test]
async fn alloc_editor_shows_add_rule_form_with_prefilled_counterparty() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Pick a transaction that has a counterparty (Mercadona)
    let (txn_id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_transactions WHERE counterparty = 'Mercadona' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/allocations"))
        .await;
    let body = response.text();

    // The "Add Rule" section is present
    assert!(body.contains("alloc-rule-form"));
    assert!(body.contains("rule-add-form"));
    // Pattern is pre-filled with counterparty value
    assert!(body.contains(r#"value="Mercadona""#));
    // Counterparty option is selected by default
    assert!(body.contains(r#"value="counterparty" selected"#));
    // The form POSTs to the rules/create endpoint
    assert!(body.contains(&format!("/leanfin/transactions/{txn_id}/rules/create")));
}

#[tokio::test]
async fn rule_create_returns_editor_with_flash_message() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (txn_id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_transactions WHERE counterparty = 'Mercadona' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/leanfin/transactions/{txn_id}/rules/create"))
        .form(&serde_json::json!({
            "label_id": label_id,
            "field": "counterparty",
            "pattern": "Mercadona"
        }))
        .await;
    let body = response.text();

    // Flash message confirms the rule was created
    assert!(body.contains("alloc-flash"));
    assert!(body.contains("Rule created"));
    // Still returns the allocation editor
    assert!(body.contains("alloc-editor"));
}

#[tokio::test]
async fn rule_create_auto_allocates_matching_unallocated_transactions() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Get an account to insert transactions into
    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Delete existing rules so we start clean for this specific pattern
    sqlx::query("DELETE FROM leanfin_label_rules")
        .execute(&app.pool)
        .await
        .unwrap();

    // Delete existing allocations so transactions are unallocated
    sqlx::query("DELETE FROM leanfin_allocations")
        .execute(&app.pool)
        .await
        .unwrap();

    // Insert two unallocated transactions with matching counterparty
    for i in 0..2 {
        sqlx::query(
            "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, counterparty) VALUES (?, ?, '2025-06-01', -15.00, 'EUR', 'Test purchase', 'UniqueVendor')",
        )
        .bind(account_id)
        .bind(format!("rule-test-{i}"))
        .execute(&app.pool)
        .await
        .unwrap();
    }

    // Pick one of our new transactions to create the rule from
    let (txn_id,): (i64,) = sqlx::query_as(
        "SELECT id FROM leanfin_transactions WHERE counterparty = 'UniqueVendor' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Create the rule — this should auto-allocate all matching transactions
    app.server
        .post(&format!("/leanfin/transactions/{txn_id}/rules/create"))
        .form(&serde_json::json!({
            "label_id": label_id,
            "field": "counterparty",
            "pattern": "UniqueVendor"
        }))
        .await;

    // Verify: the two UniqueVendor transactions now have allocations
    let (alloc_count,): (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM leanfin_allocations al
           JOIN leanfin_transactions t ON al.transaction_id = t.id
           WHERE t.counterparty = 'UniqueVendor' AND al.label_id = ?"#,
    )
    .bind(label_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(alloc_count, 2);
}

#[tokio::test]
async fn rule_create_with_invalid_label_id_does_not_create_rule() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (txn_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_transactions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let (rule_count_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_rules")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Use a label_id that doesn't belong to the user (non-existent)
    let response = app
        .server
        .post(&format!("/leanfin/transactions/{txn_id}/rules/create"))
        .form(&serde_json::json!({
            "label_id": 999999,
            "field": "counterparty",
            "pattern": "SomeVendor"
        }))
        .await;
    let body = response.text();

    // Returns the editor without a flash message
    assert!(body.contains("alloc-editor") || body.is_empty());
    assert!(!body.contains("alloc-flash"));

    let (rule_count_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_rules")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    assert_eq!(rule_count_before, rule_count_after);
}

#[tokio::test]
async fn rule_create_with_invalid_field_does_not_create_rule() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (txn_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_transactions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (rule_count_before,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_rules")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Use an invalid field value
    let response = app
        .server
        .post(&format!("/leanfin/transactions/{txn_id}/rules/create"))
        .form(&serde_json::json!({
            "label_id": label_id,
            "field": "invalid_field",
            "pattern": "SomePattern"
        }))
        .await;
    let body = response.text();

    // Returns the editor without a flash message
    assert!(body.contains("alloc-editor"));
    assert!(!body.contains("alloc-flash"));

    let (rule_count_after,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leanfin_label_rules")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    assert_eq!(rule_count_before, rule_count_after);
}

// ── Transaction details (raw API payload) ───────────────────

#[tokio::test]
async fn txn_details_returns_json_viewer_when_payload_exists() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Pick a transaction — seed data now includes API payloads
    let (txn_id, external_id): (i64, String) =
        sqlx::query_as("SELECT id, external_id FROM leanfin_transactions LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/details"))
        .await;
    let body = response.text();

    // Contains the json-viewer component
    assert!(body.contains(r#"class="json-viewer""#));
    // Contains the txn-details wrapper
    assert!(body.contains(r#"class="txn-details""#));
    // Contains the section title
    assert!(body.contains("Raw API payload"));
    // Contains the transaction_id from the payload
    assert!(body.contains(&external_id));
    // Does NOT contain the "not found" message
    assert!(!body.contains("No raw payload found"));
}

#[tokio::test]
async fn txn_details_returns_not_found_when_no_payload_exists() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Insert a transaction with no matching API payload
    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description) VALUES (?, 'no_payload_ext', '2026-01-01', -10.0, 'EUR', 'Orphan txn')"
    )
    .bind(account_id)
    .execute(&app.pool)
    .await
    .unwrap();
    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE external_id = 'no_payload_ext'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/details"))
        .await;
    let body = response.text();

    // Contains the txn-details wrapper
    assert!(body.contains(r#"class="txn-details""#));
    // Contains the section title
    assert!(body.contains("Raw API payload"));
    // Contains the "not found" message
    assert!(body.contains("No raw payload found"));
    // Does NOT contain the json-viewer component
    assert!(!body.contains(r#"class="json-viewer""#));
}

#[tokio::test]
async fn txn_details_matches_by_entry_reference() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Create a transaction with a unique external_id not present in seed payloads
    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let ext_id = "entryref_only_txn";
    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description) VALUES (?, ?, '2026-01-01', -99.99, 'EUR', 'Entry ref test')"
    )
    .bind(account_id)
    .bind(ext_id)
    .execute(&app.pool)
    .await
    .unwrap();
    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE external_id = ?")
            .bind(ext_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Insert payload that only has entry_reference (no transaction_id)
    let payload_body = serde_json::json!({
        "transactions": [
            {
                "entry_reference": ext_id,
                "amount": -99.99,
                "note": "Matched via entry_reference"
            }
        ]
    });
    sqlx::query(
        r#"INSERT INTO leanfin_api_payloads (account_id, provider, method, endpoint, status_code, duration_ms, response_body)
           VALUES (?, 'enable_banking', 'GET', '/accounts/{uid}/transactions', 200, 100, ?)"#,
    )
    .bind(account_id)
    .bind(payload_body.to_string())
    .execute(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/details"))
        .await;
    let body = response.text();

    assert!(body.contains(r#"class="json-viewer""#));
    assert!(body.contains("Matched via entry_reference"));
}

#[tokio::test]
async fn alloc_editor_has_more_details_button() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (txn_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_transactions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/allocations"))
        .await;
    let body = response.text();

    // The "More details" button is present
    assert!(body.contains("More details"));
    // It targets the details container
    assert!(body.contains(&format!("hx-target=\"#txn-details-{txn_id}\"")));
    // It fetches from the details endpoint
    assert!(body.contains(&format!("/leanfin/transactions/{txn_id}/details")));
}

// ── Zero-amount allocation support ─────────────────────────

#[tokio::test]
async fn alloc_add_allows_zero_amount_on_zero_transaction() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Insert a zero-amount transaction
    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, counterparty) VALUES (?, 'zero-txn', '2025-06-01', 0.00, 'EUR', 'Zero amount transfer', 'Bank')"
    )
    .bind(account_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE external_id = 'zero-txn'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Groceries'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Add a EUR 0.00 allocation — should succeed on a zero-amount transaction
    app.server
        .post(&format!("/leanfin/transactions/{txn_id}/allocations/add"))
        .form(&serde_json::json!({
            "label_id": label_id,
            "amount": 0.00
        }))
        .await;

    // Verify the allocation was created
    let (alloc_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM leanfin_allocations WHERE transaction_id = ?")
            .bind(txn_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    assert_eq!(
        alloc_count, 1,
        "zero-amount allocation should be created on zero-amount transaction"
    );
}

#[tokio::test]
async fn alloc_add_rejects_zero_amount_on_nonzero_transaction() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Pick a non-zero transaction from seed data
    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE ABS(amount) > 0.01 LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (label_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_labels WHERE name = 'Entertainment'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Clear any existing allocations for this transaction
    sqlx::query("DELETE FROM leanfin_allocations WHERE transaction_id = ?")
        .bind(txn_id)
        .execute(&app.pool)
        .await
        .unwrap();

    // Try to add a EUR 0.00 allocation on a non-zero transaction — should be rejected
    app.server
        .post(&format!("/leanfin/transactions/{txn_id}/allocations/add"))
        .form(&serde_json::json!({
            "label_id": label_id,
            "amount": 0.00
        }))
        .await;

    // Verify no allocation was created
    let (alloc_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM leanfin_allocations WHERE transaction_id = ? AND label_id = ?",
    )
    .bind(txn_id)
    .bind(label_id)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    assert_eq!(
        alloc_count, 0,
        "zero-amount allocation should be rejected on non-zero transaction"
    );
}

#[tokio::test]
async fn alloc_editor_shows_min_amount_zero_for_zero_transaction() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_accounts LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Insert a zero-amount transaction
    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description) VALUES (?, 'zero-min-test', '2025-06-01', 0.00, 'EUR', 'Zero test')"
    )
    .bind(account_id)
    .execute(&app.pool)
    .await
    .unwrap();

    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE external_id = 'zero-min-test'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/allocations"))
        .await;
    let body = response.text();

    // For zero-amount transactions, the min attribute on the amount input should be "0.00"
    assert!(
        body.contains(r#"min="0.00""#),
        "min amount should be 0.00 for zero transactions"
    );
    // The total should show 0.00
    assert!(body.contains("0.00</span>"));
}

#[tokio::test]
async fn alloc_editor_shows_min_amount_positive_for_nonzero_transaction() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Pick a non-zero transaction
    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE ABS(amount) > 0.01 LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/allocations"))
        .await;
    let body = response.text();

    // For non-zero transactions, the min attribute should be "0.01"
    assert!(
        body.contains(r#"min="0.01""#),
        "min amount should be 0.01 for non-zero transactions"
    );
}

// ── Account color on transaction rows ───────────────────────

#[tokio::test]
async fn transaction_row_includes_account_color_style() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Set a color on Santander account
    sqlx::query("UPDATE leanfin_accounts SET color = '#e74c3c' WHERE bank_name = 'Santander'")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(
        body.contains("--account-color:#e74c3c"),
        "transaction rows should include --account-color CSS variable from account"
    );
}

#[tokio::test]
async fn transaction_row_omits_color_style_when_no_account_color() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Ensure no accounts have a color set
    sqlx::query("UPDATE leanfin_accounts SET color = NULL")
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(
        !body.contains("--account-color"),
        "transaction rows should not include --account-color when account has no color"
    );
}

#[tokio::test]
async fn transaction_row_has_unallocated_class() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Ensure at least one transaction is unallocated
    let (txn_id,): (i64,) = sqlx::query_as("SELECT id FROM leanfin_transactions LIMIT 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM leanfin_allocations WHERE transaction_id = ?")
        .bind(txn_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(
        body.contains("txn-unallocated"),
        "unallocated transactions should have txn-unallocated class"
    );
}

#[tokio::test]
async fn single_transaction_row_refresh_includes_account_color() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Set color on an account and get a transaction from it
    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    sqlx::query("UPDATE leanfin_accounts SET color = '#2ecc71' WHERE id = ?")
        .bind(account_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let (txn_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_transactions WHERE account_id = ? LIMIT 1")
            .bind(account_id)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get(&format!("/leanfin/transactions/{txn_id}/row"))
        .await;
    let body = response.text();
    assert!(
        body.contains("--account-color:#2ecc71"),
        "single row refresh should include --account-color CSS variable"
    );
}
