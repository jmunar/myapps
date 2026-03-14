use crate::harness;

// ── Pagination ──────────────────────────────────────────────

#[tokio::test]
async fn transaction_list_shows_pagination_controls() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    // label_ids select filter exists in the dashboard
    assert!(body.contains(r#"name="label_ids""#));
    assert!(body.contains("All labels"));
    assert!(body.contains("Groceries"));
}

#[tokio::test]
async fn dashboard_nav_includes_expenses_tab() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();

    assert!(body.contains("/leanfin/expenses"));
    assert!(body.contains("Expenses"));
}

// ── Transaction label_ids filter ─────────────────────────────

#[tokio::test]
async fn transaction_label_ids_filter_returns_matching() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin").await;
    let body = response.text();
    assert!(body.contains("Transactions"));
    assert!(body.contains("hx-get"));
}

#[tokio::test]
async fn transaction_list_returns_table() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("Mercadona"));
    assert!(body.contains("Netflix"));
}

#[tokio::test]
async fn transaction_search_filters_results() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    assert!(body.contains("<th>Balance</th>"));
}

#[tokio::test]
async fn transaction_balance_shows_value_when_present() {
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

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
    let app = harness::spawn_app().await;
    app.seed_and_login().await;

    let response = app.server.get("/leanfin/transactions").await;
    let body = response.text();
    // Seed data has no balance_after set, so all balances should show "—"
    assert!(body.contains(r#"class="txn-balance">"#));
    assert!(body.contains(">\u{2014}</td>"));
}
