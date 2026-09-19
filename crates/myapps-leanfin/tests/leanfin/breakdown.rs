// ── Breakdown page (was: Expenses) ───────────────────────────

use myapps_leanfin::LeanFinApp;

async fn group_id(pool: &sqlx::SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM leanfin_label_groups WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("group {name} not seeded: {e}"))
}

#[tokio::test]
async fn breakdown_page_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let response = app.server.get("/leanfin/breakdown").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn expenses_url_redirects_to_breakdown() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let response = app.server.get("/leanfin/expenses").expect_failure().await;
    assert_eq!(response.status_code(), 308);
    assert!(
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/leanfin/breakdown")
    );
}

#[tokio::test]
async fn breakdown_page_renders_group_pills_and_period_selector() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app.server.get("/leanfin/breakdown").await.text();

    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Breakdown"));
    // Groups, not labels, are what you pick here.
    assert!(body.contains("data-group-id="));
    assert!(body.contains("Essentials"));
    assert!(body.contains("Lifestyle"));
    // The default group is shown under its translated name.
    assert!(body.contains("No group"));
    // Individual labels are no longer selectable on this page.
    assert!(!body.contains("data-label-id="));

    assert!(body.contains("period-selector"));
    for d in ["30d", "90d", "180d", "365d"] {
        assert!(body.contains(d));
    }
    assert!(body.contains(r#"class="period-btn period-btn-active" data-days="90""#));
}

#[tokio::test]
async fn breakdown_page_has_both_canvases() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app.server.get("/leanfin/breakdown").await.text();

    assert!(body.contains(r#"id="breakdown-canvas""#));
    assert!(body.contains(r#"id="breakdown-categories-canvas""#));
    assert!(body.contains("chart-container"));
    assert!(body.contains("updateBreakdown"));
    assert!(body.contains("showBreakdownEmpty"));
    // The category chart is horizontal.
    assert!(body.contains("indexAxis: 'y'"));
}

#[tokio::test]
async fn breakdown_page_empty_state_when_no_labels() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.login_as("test", "pass").await;

    let body = app.server.get("/leanfin/breakdown").await.text();

    assert!(body.contains("No labels yet. Create labels and allocate transactions first."));
    assert!(body.contains("empty-state"));
}

// ── Chart endpoint ───────────────────────────────────────────

#[tokio::test]
async fn chart_endpoint_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let response = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", "1")
        .add_query_param("days", "90")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn chart_endpoint_rejects_a_group_the_user_does_not_own() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", "99999")
        .add_query_param("days", "90")
        .await
        .text();

    assert!(body.contains("showBreakdownEmpty("));
    assert!(body.contains("Group not found."));
}

#[tokio::test]
async fn chart_endpoint_reports_an_empty_group() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    // The seed leaves "Other" with no labels in it.
    let other = group_id(&app.pool, "No group").await;
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &other.to_string())
        .add_query_param("days", "90")
        .await
        .text();

    assert!(body.contains("showBreakdownEmpty("));
    assert!(body.contains("This group has no labels yet."));
}

#[tokio::test]
async fn chart_endpoint_returns_one_payload_covering_both_charts() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let essentials = group_id(&app.pool, "Essentials").await;
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &essentials.to_string())
        .add_query_param("days", "365")
        .await
        .text();

    assert!(body.contains("updateBreakdown("));
    // Every key the page's JS reads must be present.
    for key in [
        "\"dates\"",
        "\"categories\"",
        "\"matrix\"",
        "\"windowStart\"",
        "\"color\"",
        "\"groupName\"",
    ] {
        assert!(body.contains(key), "payload is missing {key}");
    }
    // Categories are the labels of that group only.
    assert!(body.contains("Groceries"));
    assert!(!body.contains("Dining"));
}

#[tokio::test]
async fn chart_endpoint_defaults_to_90_days() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let essentials = group_id(&app.pool, "Essentials").await;
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &essentials.to_string())
        .await
        .text();

    // A 200 proves nothing here: a broken query renders the empty state with a
    // 200 too. Assert the payload, and that no empty state was rendered.
    assert!(body.contains("updateBreakdown("));
    assert!(!body.contains("showBreakdownEmpty("));
}

#[tokio::test]
async fn chart_payload_matrix_matches_the_category_count() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let essentials = group_id(&app.pool, "Essentials").await;
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &essentials.to_string())
        .add_query_param("days", "30")
        .await
        .text();

    let json = body
        .trim_start_matches("<script>window.updateBreakdown(")
        .trim_end_matches(");</script>");
    let payload: serde_json::Value = serde_json::from_str(json).expect("payload is not valid JSON");

    let categories = payload["categories"].as_array().unwrap().len();
    let dates = payload["dates"].as_array().unwrap().len();
    let matrix = payload["matrix"].as_array().unwrap();

    assert_eq!(matrix.len(), categories, "one matrix row per category");
    for row in matrix {
        assert_eq!(row.as_array().unwrap().len(), dates, "one column per date");
    }
}

// ── Bucketing boundaries ─────────────────────────────────────
//
// The payload's `matrix` is a lookup of (bucket date, label) built from two
// independent calculations: `downsample_expenses` decides which date a
// transaction is filed under, and `generate_window_dates` decides which dates
// exist. If those two disagree by a day at either end of the window, money
// silently vanishes from the chart and the endpoint still answers 200 — so
// these assert the totals, not the status.

use chrono::{Datelike, Duration, NaiveDate, Utc};

/// A user with one account, one group, one label and nothing else, so every
/// figure in the payload is one this test put there.
struct Fixture {
    account: i64,
    group: i64,
    label: i64,
}

async fn fresh_fixture(app: &myapps_test_harness::TestApp) -> Fixture {
    app.login_as("bucketuser", "pass").await;

    let user: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'bucketuser'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let account: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_accounts (user_id, bank_name, bank_country, session_id, account_uid,
             session_expires_at, account_type, account_name, balance_currency)
         VALUES (?, 'Bucket Bank', '', '', 'bucket-uid', '9999-12-31T00:00:00Z', 'manual',
                 'Bucket Bank', 'EUR') RETURNING id",
    )
    .bind(user)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let group: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_label_groups (user_id, name) VALUES (?, 'Buckets') RETURNING id",
    )
    .bind(user)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let label: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_labels (user_id, name, group_id) VALUES (?, 'Bucketed', ?) RETURNING id",
    )
    .bind(user)
    .bind(group)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    Fixture {
        account,
        group,
        label,
    }
}

/// A spend of `amount` on `date`, fully allocated to the fixture's label.
async fn spend_on(app: &myapps_test_harness::TestApp, f: &Fixture, date: NaiveDate, amount: f64) {
    let date = date.format("%Y-%m-%d").to_string();
    let txn: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency,
             description, counterparty)
         VALUES (?, ?, ?, ?, 'EUR', 'Bucket probe', 'Probe') RETURNING id",
    )
    .bind(f.account)
    .bind(format!("bucket-{date}-{amount}"))
    .bind(&date)
    .bind(-amount)
    .fetch_one(&app.pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, ?)",
    )
    .bind(txn)
    .bind(f.label)
    .bind(amount)
    .execute(&app.pool)
    .await
    .unwrap();
}

async fn payload(app: &myapps_test_harness::TestApp, group: i64, days: i64) -> serde_json::Value {
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &group.to_string())
        .add_query_param("days", &days.to_string())
        .await
        .text();

    assert!(
        body.contains("updateBreakdown("),
        "expected a payload for {days}d, got: {body}"
    );
    let json = body
        .trim_start_matches("<script>window.updateBreakdown(")
        .trim_end_matches(");</script>");
    serde_json::from_str(json).expect("payload is not valid JSON")
}

fn matrix_total(payload: &serde_json::Value) -> f64 {
    payload["matrix"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap())
        .map(|v| v.as_f64().unwrap())
        .sum()
}

fn dates(payload: &serde_json::Value) -> Vec<NaiveDate> {
    payload["dates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| NaiveDate::parse_from_str(d.as_str().unwrap(), "%Y-%m-%d").unwrap())
        .collect()
}

#[tokio::test]
async fn daily_buckets_cover_every_day_of_the_window() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    let p = payload(&app, f.group, 30).await;
    let d = dates(&p);

    let today = Utc::now().date_naive();
    assert_eq!(d.len(), 31, "30d is inclusive at both ends");
    assert_eq!(d[0], today - Duration::days(30));
    assert_eq!(*d.last().unwrap(), today);
    for pair in d.windows(2) {
        assert_eq!(pair[1], pair[0] + Duration::days(1), "days must be dense");
    }
}

#[tokio::test]
async fn weekly_buckets_all_land_on_a_sunday() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    // 31 is the first day past the daily threshold, 90 the last weekly one.
    for days in [31, 90] {
        let p = payload(&app, f.group, days).await;
        let d = dates(&p);
        for date in &d {
            assert_eq!(
                date.weekday(),
                chrono::Weekday::Sun,
                "{days}d bucket {date} is not a week end"
            );
        }
        for pair in d.windows(2) {
            assert_eq!(pair[1], pair[0] + Duration::days(7));
        }
        assert!(
            *d.last().unwrap() >= Utc::now().date_naive(),
            "the current, incomplete week must still have a bucket"
        );
    }
}

#[tokio::test]
async fn monthly_buckets_all_land_on_a_month_end() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    // 91 is the first day past the weekly threshold.
    for days in [91, 365] {
        let p = payload(&app, f.group, days).await;
        let d = dates(&p);
        for date in &d {
            assert_ne!(
                (*date + Duration::days(1)).month(),
                date.month(),
                "{days}d bucket {date} is not a month end"
            );
        }
        assert_eq!(
            d.last().unwrap().month(),
            Utc::now().date_naive().month(),
            "the current month must have a bucket"
        );
    }
}

#[tokio::test]
async fn a_spend_on_the_first_day_of_the_window_is_not_dropped() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    for days in [30i64, 90, 365] {
        // Fresh label per window so the totals never mix.
        spend_on(&app, &f, today - Duration::days(days), 100.0 + days as f64).await;
    }

    // Each window must contain its own boundary spend and every later one.
    for days in [30i64, 90, 365] {
        let expected: f64 = [30i64, 90, 365]
            .iter()
            .filter(|d| **d <= days)
            .map(|d| 100.0 + *d as f64)
            .sum();
        let total = matrix_total(&payload(&app, f.group, days).await);
        assert!(
            (total - expected).abs() < 0.01,
            "{days}d window lost money at its edge: expected {expected}, charted {total}"
        );
    }
}

#[tokio::test]
async fn a_spend_from_today_is_not_dropped() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 42.50).await;

    for days in [30, 90, 365] {
        let total = matrix_total(&payload(&app, f.group, days).await);
        assert!(
            (total - 42.50).abs() < 0.01,
            "{days}d window lost today's spend: charted {total}"
        );
    }
}

#[tokio::test]
async fn a_spend_before_the_window_is_excluded() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    spend_on(&app, &f, today, 20.0).await;
    spend_on(&app, &f, today - Duration::days(31), 999.0).await;

    let total = matrix_total(&payload(&app, f.group, 30).await);
    assert!(
        (total - 20.0).abs() < 0.01,
        "a spend older than the window was charted: {total}"
    );
}

#[tokio::test]
async fn window_start_precedes_the_first_bucket() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    // The JS reads the first bucket's period as windowStart..dates[0], so a
    // windowStart after dates[0] would make that period empty.
    for days in [30i64, 90, 365] {
        let p = payload(&app, f.group, days).await;
        let start =
            NaiveDate::parse_from_str(p["windowStart"].as_str().unwrap(), "%Y-%m-%d").unwrap();
        assert_eq!(start, Utc::now().date_naive() - Duration::days(days));
        assert!(
            start <= dates(&p)[0],
            "{days}d: windowStart {start} is after the first bucket"
        );
    }
}

#[tokio::test]
async fn income_and_spending_keep_opposite_signs_in_the_matrix() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    spend_on(&app, &f, today, 30.0).await;

    // A credit allocated to the same label: the series counts it negatively.
    let txn: i64 = sqlx::query_scalar(
        "INSERT INTO leanfin_transactions (account_id, external_id, date, amount, currency,
             description, counterparty)
         VALUES (?, 'bucket-credit', ?, 10.0, 'EUR', 'Refund', 'Probe') RETURNING id",
    )
    .bind(f.account)
    .bind(today.format("%Y-%m-%d").to_string())
    .fetch_one(&app.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO leanfin_allocations (transaction_id, label_id, amount) VALUES (?, ?, 10.0)",
    )
    .bind(txn)
    .bind(f.label)
    .execute(&app.pool)
    .await
    .unwrap();

    let total = matrix_total(&payload(&app, f.group, 30).await);
    assert!(
        (total - 20.0).abs() < 0.01,
        "a refund must net off against the spend: charted {total}"
    );
}

#[tokio::test]
async fn chart_endpoint_reports_a_group_whose_labels_have_no_data() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    // The group has a label, but nothing was ever allocated to it.
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &f.group.to_string())
        .add_query_param("days", "90")
        .await
        .text();

    assert!(body.contains("showBreakdownEmpty("));
    assert!(body.contains("No expense data for this group in this period."));
    // Distinct from the "group has no labels" message, which is a different fix.
    assert!(!body.contains("This group has no labels yet."));
}

#[tokio::test]
async fn expenses_redirect_requires_authentication() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;

    let response = app.server.get("/leanfin/expenses").expect_failure().await;
    assert_eq!(
        response.status_code(),
        303,
        "the legacy URL must go to login, not straight to /breakdown"
    );
}
