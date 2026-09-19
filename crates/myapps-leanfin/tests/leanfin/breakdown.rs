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
