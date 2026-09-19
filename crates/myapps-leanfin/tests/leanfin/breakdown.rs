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
async fn breakdown_page_renders_group_pills_and_window_selector() {
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

    // One stepped box rather than one button per window, plus the `+` that
    // adds the period we are currently in.
    assert!(body.contains(r#"data-window="10w""#));
    assert!(body.contains(r#"<span class="lf-window-value">10w</span>"#));
    assert!(body.contains(r#"data-step="longer""#));
    assert!(body.contains(r#"class="lf-window-now lf-window-now-active" aria-pressed="true""#));
    assert!(!body.contains("period-btn"));
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
        .add_query_param("window", "10w")
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
        .add_query_param("window", "10w")
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
        .add_query_param("window", "10w")
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
        .add_query_param("window", "12m")
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
async fn chart_endpoint_defaults_to_the_default_window() {
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
        .add_query_param("window", "30d")
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

async fn payload(
    app: &myapps_test_harness::TestApp,
    group: i64,
    window: &str,
) -> serde_json::Value {
    payload_with(app, group, window, "1").await
}

async fn payload_with(
    app: &myapps_test_harness::TestApp,
    group: i64,
    window: &str,
    current: &str,
) -> serde_json::Value {
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &group.to_string())
        .add_query_param("window", window)
        .add_query_param("current", current)
        .await
        .text();

    assert!(
        body.contains("updateBreakdown("),
        "expected a payload for {window}, got: {body}"
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

fn iso_dates(value: &serde_json::Value) -> Vec<NaiveDate> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|d| NaiveDate::parse_from_str(d.as_str().unwrap(), "%Y-%m-%d").unwrap())
        .collect()
}

/// The start of each bucket — the day after the previous bucket ended.
fn starts(payload: &serde_json::Value) -> Vec<NaiveDate> {
    iso_dates(&payload["starts"])
}

/// The end of each bucket, which is what the chart plots a bar against.
fn dates(payload: &serde_json::Value) -> Vec<NaiveDate> {
    iso_dates(&payload["dates"])
}

#[tokio::test]
async fn daily_buckets_cover_every_day_of_the_window() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    let p = payload(&app, f.group, "30d").await;
    let d = dates(&p);

    let today = Utc::now().date_naive();
    // 30 whole days that have run their course, plus the one we are in.
    assert_eq!(d.len(), 31);
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

    let p = payload(&app, f.group, "10w").await;
    let d = dates(&p);
    assert_eq!(d.len(), 11, "ten whole weeks, plus the one we are in");
    for date in &d {
        assert_eq!(
            date.weekday(),
            chrono::Weekday::Sun,
            "bucket {date} is not a week end"
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

#[tokio::test]
async fn monthly_buckets_all_land_on_a_month_end() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    for (window, months) in [("6m", 7), ("12m", 13)] {
        let p = payload(&app, f.group, window).await;
        let d = dates(&p);
        assert_eq!(d.len(), months, "{window} should chart {months} buckets");
        for date in &d {
            assert_ne!(
                (*date + Duration::days(1)).month(),
                date.month(),
                "{window} bucket {date} is not a month end"
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
async fn the_window_opens_on_a_whole_period() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    // A month window starts on the 1st and a week window on a Monday: that is
    // what makes one bar comparable with the next.
    let p = payload(&app, f.group, "12m").await;
    assert_eq!(starts(&p)[0].day(), 1);
    let p = payload(&app, f.group, "10w").await;
    assert_eq!(starts(&p)[0].weekday(), chrono::Weekday::Mon);
}

#[tokio::test]
async fn a_spend_on_the_first_day_of_the_window_is_not_dropped() {
    for window in ["30d", "10w", "6m", "12m"] {
        let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
        let f = fresh_fixture(&app).await;
        // Something recent, so the group has data and a payload to read the
        // window's own first day off.
        spend_on(&app, &f, Utc::now().date_naive(), 1.0).await;
        let start = starts(&payload(&app, f.group, window).await)[0];

        spend_on(&app, &f, start, 100.0).await;
        let total = matrix_total(&payload(&app, f.group, window).await);
        assert!(
            (total + 101.0).abs() < 0.01,
            "{window} lost money at its edge: charted {total}"
        );
    }
}

#[tokio::test]
async fn a_spend_from_today_is_not_dropped() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 42.50).await;

    for window in ["30d", "10w", "6m", "12m"] {
        let total = matrix_total(&payload(&app, f.group, window).await);
        assert!(
            (total + 42.50).abs() < 0.01,
            "{window} lost today's spend: charted {total}"
        );
    }
}

#[tokio::test]
async fn switching_the_plus_off_drops_the_period_we_are_in() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    spend_on(&app, &f, today, 42.50).await;
    // Something in a period that has closed, so the payload is never empty.
    spend_on(&app, &f, today - Duration::days(40), 10.0).await;

    let with = payload_with(&app, f.group, "12m", "1").await;
    let without = payload_with(&app, f.group, "12m", "0").await;

    assert_eq!(dates(&with).len(), dates(&without).len() + 1);
    assert!((matrix_total(&with) + 52.50).abs() < 0.01);
    assert!(
        (matrix_total(&without) + 10.0).abs() < 0.01,
        "the running month must not be charted when the `+` is off"
    );
}

#[tokio::test]
async fn a_spend_before_the_window_is_excluded() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    spend_on(&app, &f, today, 20.0).await;
    let start = starts(&payload(&app, f.group, "30d").await)[0];
    spend_on(&app, &f, start - Duration::days(1), 999.0).await;

    let total = matrix_total(&payload(&app, f.group, "30d").await);
    assert!(
        (total + 20.0).abs() < 0.01,
        "a spend older than the window was charted: {total}"
    );
}

#[tokio::test]
async fn every_bucket_carries_the_period_it_covers() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    for window in ["30d", "10w", "6m", "12m"] {
        let p = payload(&app, f.group, window).await;
        let ends = dates(&p);
        let starts = starts(&p);
        assert_eq!(starts.len(), ends.len());
        assert_eq!(
            NaiveDate::parse_from_str(p["windowStart"].as_str().unwrap(), "%Y-%m-%d").unwrap(),
            starts[0],
            "{window}: windowStart must be the first bucket's own start"
        );
        // Periods tile the window: each starts the day after the last ended.
        for i in 0..ends.len() {
            assert!(starts[i] <= ends[i], "{window}: period {i} runs backwards");
            if i > 0 {
                assert_eq!(starts[i], ends[i - 1] + Duration::days(1));
            }
        }
    }
}

#[tokio::test]
async fn only_the_period_we_are_in_weighs_less_than_a_whole_one() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;
    // Also in a month that has closed, so the `+`-off payload is not empty.
    spend_on(&app, &f, Utc::now().date_naive() - Duration::days(40), 5.0).await;

    // The weights are what the charts divide by for a per-period average, so
    // a closed period must count as exactly one.
    let p = payload(&app, f.group, "12m").await;
    let w: Vec<f64> = serde_json::from_value(p["weights"].clone()).unwrap();
    assert_eq!(w.len(), dates(&p).len());
    assert!(w[..12].iter().all(|x| *x == 1.0), "closed months weigh 1");

    let today = Utc::now().date_naive();
    let days_in_month = today
        .with_day(1)
        .unwrap()
        .checked_add_months(chrono::Months::new(1))
        .unwrap()
        .signed_duration_since(today.with_day(1).unwrap())
        .num_days() as f64;
    assert!(
        (w[12] - today.day() as f64 / days_in_month).abs() < 1e-9,
        "the running month should weigh the days gone by: {}",
        w[12]
    );

    let off = payload_with(&app, f.group, "12m", "0").await;
    let w: Vec<f64> = serde_json::from_value(off["weights"].clone()).unwrap();
    assert!(w.iter().all(|x| *x == 1.0));
}

#[tokio::test]
async fn the_payload_names_the_average_for_its_bucket() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    for (window, label) in [
        ("30d", "Daily average"),
        ("10w", "Weekly average"),
        ("6m", "Monthly average"),
        ("12m", "Monthly average"),
    ] {
        let p = payload(&app, f.group, window).await;
        assert_eq!(p["avgLabel"], label, "{window} named its average wrong");
    }
}

#[tokio::test]
async fn spending_is_negative_and_income_positive() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let today = Utc::now().date_naive();
    spend_on(&app, &f, today, 30.0).await;

    // A credit allocated to the same label: it is income, so it counts up.
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

    // Spending reads the way it reads on a statement: 30 out, 10 in, 20 down.
    let total = matrix_total(&payload(&app, f.group, "30d").await);
    assert!(
        (total + 20.0).abs() < 0.01,
        "the net of a 30 spend and a 10 refund should be -20, charted {total}"
    );

    // And on its own, a spend is never charted as a positive bar.
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, today, 30.0).await;
    let total = matrix_total(&payload(&app, f.group, "30d").await);
    assert!((total + 30.0).abs() < 0.01, "a spend charted as {total}");
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
        .add_query_param("window", "10w")
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

// ── The config the page's script reads back out of the DOM ───
//
// `breakdown.js` takes its base path, its opening window and every string it
// ever puts on screen from the `#breakdown-controls` dataset. None of that is
// visible in the rendered page, so dropping an attribute costs the page its
// empty-state text — or its data, if `data-base` goes — with no other symptom.

#[tokio::test]
async fn the_breakdown_page_hands_its_script_the_whole_config() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    app.seed_and_login(&LeanFinApp).await;

    let body = app.server.get("/leanfin/breakdown").await.text();

    assert!(body.contains(r#"id="breakdown-controls""#));
    for attr in [
        // Where every request the script makes is rooted.
        r#"data-base=""#,
        // The window the first chart request must agree with the selector on.
        r#"data-window="10w""#,
        r#"data-current="1""#,
    ] {
        assert!(body.contains(attr), "#breakdown-controls is missing {attr}");
    }

    // The two strings the script writes into the page itself, which no other
    // assertion would catch because nothing renders them server-side.
    assert!(
        body.contains(
            r#"data-msg-select-group="Select a group to see how its spending breaks down""#
        )
    );
    assert!(body.contains(r#"data-msg-full-range="Whole period""#));

    // And the same prompt is already on screen before any group is picked.
    assert!(body.contains(r#"<div id="breakdown-empty" class="empty-state""#));
    assert!(body.contains("Select a group to see how its spending breaks down"));
}

// ── Junk in the query string ─────────────────────────────────
//
// Both parameters ride in a URL a person can edit and htmx rebuilds, so a
// value the server has never heard of must chart the default window rather
// than answer a 400 or a 500 the page has no way to show.

#[tokio::test]
async fn a_junk_window_charts_the_default_one() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    for junk in ["banana", "", "90d", "10W"] {
        let p = payload_with(&app, f.group, junk, "1").await;
        let d = dates(&p);
        // The default is ten whole weeks plus the running one.
        assert_eq!(d.len(), 11, "window={junk} did not fall back to 10w");
        for date in &d {
            assert_eq!(date.weekday(), chrono::Weekday::Sun, "window={junk}");
        }
    }
}

#[tokio::test]
async fn a_junk_current_flag_still_answers_with_a_chart() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    // In a week that has closed, so the payload is never empty even when the
    // running period is left out.
    spend_on(&app, &f, Utc::now().date_naive() - Duration::days(14), 5.0).await;

    for junk in ["banana", "", "2"] {
        let p = payload_with(&app, f.group, "10w", junk).await;
        let d = dates(&p);
        // Junk falls back to the default the same way a junk window does, so
        // it keeps the running period rather than silently dropping it: ten
        // whole weeks plus the one we are in.
        assert_eq!(d.len(), 11, "current={junk} charted the wrong buckets");
        for date in &d {
            assert_eq!(date.weekday(), chrono::Weekday::Sun, "current={junk}");
        }
    }

    // Only an explicit no turns it off.
    for off in ["0", "false"] {
        let p = payload_with(&app, f.group, "10w", off).await;
        assert_eq!(dates(&p).len(), 10, "current={off} kept the running week");
    }
}

#[tokio::test]
async fn an_unnamed_current_flag_keeps_the_running_period() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;
    spend_on(&app, &f, Utc::now().date_naive(), 5.0).await;

    // The page opens with the `+` on, so a request that does not mention the
    // flag at all must chart the period we are in — otherwise today's spending
    // disappears on the page's very first request.
    let body = app
        .server
        .get("/leanfin/breakdown/chart")
        .add_query_param("group_id", &f.group.to_string())
        .add_query_param("window", "10w")
        .await
        .text();
    let json = body
        .trim_start_matches("<script>window.updateBreakdown(")
        .trim_end_matches(");</script>");
    let p: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(dates(&p).len(), 11);
    assert!(
        (matrix_total(&p) + 5.0).abs() < 0.01,
        "today's spend was dropped"
    );
}

// ── The series query's own bounds ────────────────────────────
//
// `get_expense_series` took a day count until the stepped window replaced it
// with a pair of dates. Both ends are inclusive: the window's first and last
// days are whole periods' edges, and a transaction that lands exactly on one
// belongs inside the chart, not next to it.

#[tokio::test]
async fn the_series_query_counts_both_of_its_own_end_days() {
    let app = myapps_test_harness::spawn_app(vec![Box::new(LeanFinApp)]).await;
    let f = fresh_fixture(&app).await;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'bucketuser'")
        .fetch_one(&app.pool)
        .await
        .unwrap();

    let from = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
    let to = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
    spend_on(&app, &f, from - Duration::days(1), 1.0).await;
    spend_on(&app, &f, from, 10.0).await;
    spend_on(&app, &f, to, 20.0).await;
    spend_on(&app, &f, to + Duration::days(1), 2.0).await;

    let points = myapps_leanfin::services::expenses::get_expense_series(
        &app.pool,
        user_id,
        &[f.label],
        "2026-03-01",
        "2026-03-31",
    )
    .await
    .unwrap();

    let dates: Vec<&str> = points.iter().map(|p| p.date.as_str()).collect();
    assert_eq!(
        dates,
        vec!["2026-03-01", "2026-03-31"],
        "the bounds must take their own days and nothing outside them"
    );
    // Spending keeps its statement sign, so both come back negative.
    let total: f64 = points.iter().map(|p| p.total).sum();
    assert!((total + 30.0).abs() < 0.01, "totalled {total}");
}
