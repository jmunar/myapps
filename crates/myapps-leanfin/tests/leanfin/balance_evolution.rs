use chrono::Datelike;

#[tokio::test]
async fn balance_evolution_page_renders_with_nav_and_controls() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Balance Evolution"));
    assert!(body.contains("balance-controls"));
    assert!(body.contains("lf-window"));
}

#[tokio::test]
async fn balance_evolution_page_shows_balance_nav_active() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    // The "Balance" nav item should be marked active
    assert!(body.contains(r#"active"#));
    assert!(body.contains("/leanfin/balance-evolution"));
    assert!(body.contains("Balance"));
}

#[tokio::test]
async fn balance_evolution_page_has_all_accounts_option() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains(r#"<option value="" selected>All accounts</option>"#));
}

#[tokio::test]
async fn balance_evolution_page_has_individual_account_options() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("Santander"));
    assert!(body.contains("ING Direct"));
}

#[tokio::test]
async fn balance_evolution_page_has_a_window_selector() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    // One box, stepped through — not one button per window.
    assert!(body.contains(r#"data-window="10w""#));
    assert!(body.contains(r#"<span class="lf-window-value">10w</span>"#));
    assert!(body.contains(r#"data-step="longer""#));
    assert!(body.contains(r#"data-step="shorter""#));
    // The running period is charted until the `+` is switched off.
    assert!(body.contains(r#"class="lf-window-now lf-window-now-active" aria-pressed="true""#));
    assert!(!body.contains(">90d</button>"));
}

#[tokio::test]
async fn data_endpoint_returns_script_calling_update_balance_chart() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // Data endpoint returns a script tag calling updateBalanceChart with JSON arrays
    assert!(body.contains("updateBalanceChart("));
    assert!(body.contains("<script>"));
}

#[tokio::test]
async fn data_endpoint_returns_script_when_account_id_empty() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // Aggregated balance also returns updateBalanceChart script
    assert!(body.contains("updateBalanceChart("));
}

#[tokio::test]
async fn data_endpoint_returns_empty_state_when_no_balance_data() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Delete all balance snapshots
    sqlx::query("DELETE FROM leanfin_balance_snapshots")
        .execute(&app.pool)
        .await
        .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // Empty state is now shown via showBalanceEmpty script call
    assert!(body.contains("showBalanceEmpty("));
    assert!(body.contains("No balance data yet"));
}

#[tokio::test]
async fn data_endpoint_returns_not_found_for_other_users_account() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // Use an account ID that doesn't belong to the user
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "99999")
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // Not-found is shown via showBalanceEmpty script call
    assert!(body.contains("showBalanceEmpty("));
    assert!(body.contains("Account not found"));
}

#[tokio::test]
async fn data_endpoint_contains_balance_data_as_json_arrays() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    let p = chart_payload(&body);
    let dates = p["dates"].as_array().expect("dates array");
    assert!(!dates.is_empty());
    assert_eq!(p["values"].as_array().unwrap().len(), dates.len());
    assert_eq!(p["starts"].as_array().unwrap().len(), dates.len());
    assert!(
        chrono::NaiveDate::parse_from_str(dates[0].as_str().unwrap(), "%Y-%m-%d").is_ok(),
        "dates should be ISO days: {dates:?}"
    );
}

#[tokio::test]
async fn data_endpoint_passes_account_id_to_chart_function() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // The payload carries the account, so a click-through keeps the filter.
    assert_eq!(chart_payload(&body)["accountId"], account_id.to_string());
}

#[tokio::test]
async fn balance_page_chart_config_uses_accent_color() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // The accent color is now in the page template (Chart.js config), not the data endpoint
    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(body.contains("#1A6B5A"));
}

#[tokio::test]
async fn single_snapshot_with_historical_transactions_shows_full_series() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    // Create an account with one snapshot (today) and transactions spanning multiple days
    let user_id: (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type) VALUES (?, 'TestBank', 'ES', 'sess', 'uid_test', '2027-01-01T00:00:00Z', 'bank')"
    )
    .bind(user_id.0)
    .execute(&app.pool)
    .await
    .unwrap();

    let (account_id,): (i64,) =
        sqlx::query_as("SELECT id FROM leanfin_accounts WHERE bank_name = 'TestBank'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    // Insert a single snapshot at today with balance 1000
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let timestamp = format!("{today}T06:00:00Z");
    let snap_result = sqlx::query(
        "INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type) VALUES (?, ?, ?, 1000.0, 'ITAV')"
    )
    .bind(account_id)
    .bind(&timestamp)
    .bind(&today)
    .execute(&app.pool)
    .await
    .unwrap();
    let snap_id = snap_result.last_insert_rowid();

    // Insert transactions on earlier dates, linked to this snapshot
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let two_days_ago = (chrono::Utc::now() - chrono::Duration::days(2))
        .format("%Y-%m-%d")
        .to_string();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx1', ?, -50.0, 'EUR', 'Purchase', ?)"
    )
    .bind(account_id)
    .bind(&yesterday)
    .bind(snap_id)
    .execute(&app.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency, description, snapshot_id) VALUES (?, 'tx2', ?, -100.0, 'EUR', 'Big purchase', ?)"
    )
    .bind(account_id)
    .bind(&two_days_ago)
    .bind(snap_id)
    .execute(&app.pool)
    .await
    .unwrap();

    // Fetch balance data — should show multiple days, not just today
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "30d")
        .await;
    let body = response.text();

    // Should contain the chart update call (not empty state)
    assert!(body.contains("updateBalanceChart("), "should render chart");

    // Should contain dates from at least 2 days ago (backward walk worked)
    assert!(
        body.contains(&two_days_ago),
        "chart should include date from 2 days ago: {two_days_ago}\nbody: {body}"
    );
    assert!(
        body.contains(&yesterday),
        "chart should include yesterday: {yesterday}"
    );
    assert!(body.contains(&today), "chart should include today");
}

#[tokio::test]
async fn balance_page_chart_has_click_drill_down() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // The onClick handler and loadBalanceTxn are now in the page template
    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        body.contains("onClick"),
        "chart config should have onClick handler"
    );
    assert!(
        body.contains("loadBalanceTxn"),
        "chart should call loadBalanceTxn on click"
    );
}

#[tokio::test]
async fn balance_evolution_page_has_transaction_drill_down_card() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();
    assert!(
        body.contains("balance-txn-card"),
        "page should have hidden transaction card"
    );
    assert!(
        body.contains("balance-txn-table"),
        "page should have transaction table container"
    );
    assert!(
        body.contains("loadBalanceTxn"),
        "page should define loadBalanceTxn function"
    );
}

#[tokio::test]
async fn balance_evolution_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    // Do NOT log in
    let response = app
        .server
        .get("/leanfin/balance-evolution")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn data_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    // Do NOT log in
    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn balance_evolution_page_has_persistent_canvas_in_chart_container() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();

    // Canvas is persistent in the page template
    assert!(body.contains("<canvas"));
    assert!(body.contains(r#"id="balance-canvas""#));
    assert!(body.contains("chart-container"));
    // The updateBalanceChart function is defined in the page
    assert!(body.contains("updateBalanceChart"));
    // The showBalanceEmpty function is defined in the page
    assert!(body.contains("showBalanceEmpty"));
}

#[tokio::test]
async fn balance_evolution_page_chart_config_uses_line_type() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app.server.get("/leanfin/balance-evolution").await;
    let body = response.text();

    // Chart.js line chart configuration in the page template
    assert!(body.contains("type: 'line'"));
    assert!(body.contains("fill: true"));
}

#[tokio::test]
async fn data_endpoint_returns_empty_account_id_for_aggregated() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("window", "10w")
        .await;
    let body = response.text();
    // Aggregated: no account to carry through to the transaction list.
    assert_eq!(chart_payload(&body)["accountId"], "");
}

/// Insert a manual account with a single balance entry `days_ago` days in the past.
async fn insert_stale_manual_account(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    name: &str,
    balance: f64,
    days_ago: i64,
    archived: bool,
) -> i64 {
    let uid = format!("manual_{name}");
    let account_id = sqlx::query(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid, session_expires_at, account_type, account_name, balance_amount, balance_currency, archived) VALUES (?, ?, '', '', ?, '9999-12-31T00:00:00Z', 'manual', ?, ?, 'EUR', ?)",
    )
    .bind(user_id)
    .bind(name)
    .bind(&uid)
    .bind(name)
    .bind(balance)
    .bind(archived)
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let date = (chrono::Utc::now() - chrono::Duration::days(days_ago))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query(
        "INSERT INTO leanfin_balance_snapshots (account_id, timestamp, date, balance, balance_type) VALUES (?, ?, ?, ?, 'MANUAL')",
    )
    .bind(account_id)
    .bind(format!("{date}T23:59:59Z"))
    .bind(&date)
    .bind(balance)
    .execute(pool)
    .await
    .unwrap();

    account_id
}

#[tokio::test]
async fn manual_account_without_recent_entries_still_shows_balance() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // Last updated 200 days ago — well outside the 90-day window
    let account_id =
        insert_stale_manual_account(&app.pool, user_id, "Old Portfolio", 5000.0, 200, false).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await;
    let body = response.text();

    assert!(
        body.contains("updateBalanceChart("),
        "stale manual account should still render a series, got: {body}"
    );
    assert!(
        chart_payload(&body)["values"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.as_f64().unwrap() == 5000.0),
        "series should carry the last known balance forward, got: {body}"
    );
}

#[tokio::test]
async fn aggregated_series_includes_manual_account_without_recent_entries() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    // One account updated inside the window, one untouched for 200 days
    insert_stale_manual_account(&app.pool, user_id, "Current Account", 1000.0, 10, false).await;
    insert_stale_manual_account(&app.pool, user_id, "Old Portfolio", 5000.0, 200, false).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("window", "10w")
        .await;
    let body = response.text();

    assert!(
        chart_payload(&body)["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_f64().unwrap() == 6000.0),
        "total should include the account with no recent entries, got: {body}"
    );
}

#[tokio::test]
async fn aggregated_series_excludes_archived_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.login_as("demo", "demo").await;

    let (user_id,): (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = 'demo'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    insert_stale_manual_account(&app.pool, user_id, "Current Account", 1000.0, 10, false).await;
    insert_stale_manual_account(&app.pool, user_id, "Closed Account", 7000.0, 200, true).await;

    let response = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", "")
        .add_query_param("window", "10w")
        .await;
    let body = response.text();

    let values = chart_payload(&body)["values"].as_array().unwrap().clone();
    assert!(
        values.iter().any(|v| v.as_f64().unwrap() == 1000.0),
        "total should include the active account, got: {body}"
    );
    assert!(
        values.iter().all(|v| v.as_f64().unwrap() != 8000.0),
        "archived account should not contribute to the total, got: {body}"
    );
}

// ── Deep link from the Accounts tab ──────────────────────────
//
// The balance amount on /leanfin/accounts is an anchor to
// `?account_id=N`. The parameter arrives in a URL a person can edit, so every
// shape of junk must land on the aggregate view rather than a 400.

async fn santander_id(app: &myapps_test_harness::TestApp) -> i64 {
    sqlx::query_scalar("SELECT id FROM leanfin_accounts WHERE bank_name = 'Santander'")
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn deep_link_preselects_the_requested_account() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;
    let account_id = santander_id(&app).await;

    let body = app
        .server
        .get("/leanfin/balance-evolution")
        .add_query_param("account_id", &account_id.to_string())
        .await
        .text();

    assert!(
        body.contains(&format!(r#"<option value="{account_id}" selected>"#)),
        "the linked account should be selected in the picker"
    );
    assert!(
        !body.contains(r#"<option value="" selected>"#),
        "All accounts must not also be selected"
    );
    // The first data request must already be filtered, or the page would flash
    // the aggregate series before anyone touched the picker.
    assert!(body.contains(&format!("account_id={account_id}&window=10w&current=1")));
}

#[tokio::test]
async fn deep_link_with_a_junk_account_id_falls_back_to_all_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    for junk in ["not-a-number", "", "9e99", "-1"] {
        let response = app
            .server
            .get("/leanfin/balance-evolution")
            .add_query_param("account_id", junk)
            .await;
        assert_eq!(
            response.status_code(),
            200,
            "account_id={junk} was rejected"
        );
        assert!(
            response
                .text()
                .contains(r#"<option value="" selected>All accounts</option>"#),
            "account_id={junk} should fall back to the aggregate view"
        );
    }
}

#[tokio::test]
async fn deep_link_to_an_archived_account_falls_back_to_all_accounts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    // BBVA is seeded archived; it has no <option>, so it cannot be selected.
    let archived: i64 =
        sqlx::query_scalar("SELECT id FROM leanfin_accounts WHERE bank_name = 'BBVA'")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let body = app
        .server
        .get("/leanfin/balance-evolution")
        .add_query_param("account_id", &archived.to_string())
        .await
        .text();

    assert!(body.contains(r#"<option value="" selected>All accounts</option>"#));
    assert!(
        !body.contains(&format!(r#"<option value="{archived}""#)),
        "an archived account must not appear in the picker"
    );
    assert!(body.contains("account_id=&window=10w&current=1"));
}

// ── Bucket boundaries in the payload ─────────────────────────

/// The single `updateBalanceChart({...})` payload in a response.
fn chart_payload(body: &str) -> serde_json::Value {
    let call = body
        .split_once("updateBalanceChart(")
        .unwrap_or_else(|| panic!("no chart call in: {body}"))
        .1;
    let json = call
        .rsplit_once(");</script>")
        .expect("unterminated call")
        .0;
    serde_json::from_str(json).expect("payload is not valid JSON")
}

#[tokio::test]
async fn every_point_carries_the_start_of_the_period_it_closes() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;
    let account_id = santander_id(&app).await;

    let body = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "10w")
        .await
        .text();

    let p = chart_payload(&body);
    let starts: Vec<String> = serde_json::from_value(p["starts"].clone()).unwrap();
    let dates: Vec<String> = serde_json::from_value(p["dates"].clone()).unwrap();
    assert_eq!(starts.len(), dates.len());

    // A point is the END of a period, and clicking it lists the transactions
    // from its start — so a start after its own end would list nothing.
    for (start, end) in starts.iter().zip(&dates) {
        assert!(start <= end, "period {start}..{end} runs backwards");
    }
    // Weekly buckets: each period starts the day after the one before ends.
    for pair in dates.windows(2) {
        let prev = chrono::NaiveDate::parse_from_str(&pair[0], "%Y-%m-%d").unwrap();
        let next = chrono::NaiveDate::parse_from_str(&pair[1], "%Y-%m-%d").unwrap();
        assert_eq!(next, prev + chrono::Duration::days(7));
    }
}

#[tokio::test]
async fn a_twelve_month_window_is_bucketed_by_month() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;
    let account_id = santander_id(&app).await;

    let body = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &account_id.to_string())
        .add_query_param("window", "12m")
        .await
        .text();

    let p = chart_payload(&body);
    let dates: Vec<String> = serde_json::from_value(p["dates"].clone()).unwrap();
    let starts: Vec<String> = serde_json::from_value(p["starts"].clone()).unwrap();

    assert!(
        dates.len() > 1,
        "a year of seeded history should produce several monthly points"
    );
    for (start, end) in starts.iter().zip(&dates) {
        let start = chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap();
        let end = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d").unwrap();
        assert_eq!(start.day(), 1, "a monthly bucket starts on the 1st");
        assert_ne!(
            (end + chrono::Duration::days(1)).month(),
            end.month(),
            "a monthly bucket ends on a month end"
        );
    }
}

#[tokio::test]
async fn the_running_period_is_dropped_when_the_plus_is_off() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;
    let account_id = santander_id(&app).await;

    let with: Vec<String> = serde_json::from_value(
        chart_payload(
            &app.server
                .get("/leanfin/balance-evolution/data")
                .add_query_param("account_id", &account_id.to_string())
                .add_query_param("window", "12m")
                .add_query_param("current", "1")
                .await
                .text(),
        )["dates"]
            .clone(),
    )
    .unwrap();
    let without: Vec<String> = serde_json::from_value(
        chart_payload(
            &app.server
                .get("/leanfin/balance-evolution/data")
                .add_query_param("account_id", &account_id.to_string())
                .add_query_param("window", "12m")
                .add_query_param("current", "0")
                .await
                .text(),
        )["dates"]
            .clone(),
    )
    .unwrap();

    // The running month is the only difference, and it is the newest bucket.
    assert_eq!(with.len(), without.len() + 1);
    assert_eq!(&with[..without.len()], &without[..]);
    let today = chrono::Utc::now().date_naive();
    let last_complete =
        chrono::NaiveDate::parse_from_str(without.last().unwrap(), "%Y-%m-%d").unwrap();
    assert!(
        last_complete < today.with_day(1).unwrap(),
        "the last complete month must end before the running one begins"
    );
}

#[tokio::test]
async fn window_start_is_blank_when_there_is_no_series() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(myapps_leanfin::LeanFinApp)]).await;
    app.seed_and_login(&myapps_leanfin::LeanFinApp).await;

    sqlx::query("DELETE FROM leanfin_balance_snapshots")
        .execute(&app.pool)
        .await
        .unwrap();

    let body = app
        .server
        .get("/leanfin/balance-evolution/data")
        .add_query_param("account_id", &santander_id(&app).await.to_string())
        .add_query_param("window", "10w")
        .await
        .text();

    // No series means the empty-state call, never a chart call with a blank date.
    assert!(body.contains("showBalanceEmpty("));
    assert!(!body.contains("updateBalanceChart("));
}
