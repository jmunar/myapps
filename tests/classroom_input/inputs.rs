use crate::harness;

#[tokio::test]
async fn inputs_page_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/classroom").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn inputs_page_renders_seeded_inputs() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let response = app.server.get("/classroom").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Week 10 quiz"));
    assert!(body.contains("Week 11 quiz"));
    assert!(body.contains("Attendance"));
    assert!(body.contains("Reading assessment"));
}

#[tokio::test]
async fn inputs_page_shows_classroom_labels() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let response = app.server.get("/classroom").await;
    let body = response.text();
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    assert!(body.contains("2-A"));
}

#[tokio::test]
async fn inputs_page_has_new_input_button() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let response = app.server.get("/classroom").await;
    let body = response.text();
    assert!(body.contains("/classroom/new"));
}

#[tokio::test]
async fn new_input_page_requires_authentication() {
    let app = harness::spawn_app().await;
    let response = app.server.get("/classroom/new").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn new_input_page_renders_with_dropdowns() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let response = app.server.get("/classroom/new").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("New Input"));
    assert!(body.contains(r#"name="classroom_id""#));
    assert!(body.contains(r#"name="form_type_id""#));
    assert!(body.contains(r#"name="name""#));
    // Should list classrooms
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    // Should list form types
    assert!(body.contains("Weekly quiz"));
    assert!(body.contains("Attendance"));
}

#[tokio::test]
async fn view_input_detail() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM classroom_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app.server.get(&format!("/classroom/inputs/{id}")).await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("Week 10 quiz"));
    // Should contain pupil names from CSV
    assert!(body.contains("Alba"));
    assert!(body.contains("Carlos"));
}

#[tokio::test]
async fn create_input() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let (cls_id,): (i64,) =
        sqlx::query_as("SELECT id FROM classroom_input_classrooms WHERE label = '1-A' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (ft_id,): (i64,) = sqlx::query_as(
        "SELECT id FROM classroom_input_form_types WHERE name = 'Weekly quiz' LIMIT 1",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();

    let response = app
        .server
        .post("/classroom/inputs/create")
        .form(&serde_json::json!({
            "classroom_id": cls_id,
            "form_type_id": ft_id,
            "name": "Week 12 quiz",
            "csv_data": "Pupil,Score,Comment\nAlba García,9,Great\nCarlos López,8,Good",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/classroom").await;
    let body = list.text();
    assert!(body.contains("Week 12 quiz"));
}

#[tokio::test]
async fn delete_input() {
    let app = harness::spawn_app().await;
    app.seed_and_login_classroom().await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM classroom_input_inputs WHERE name = 'Week 10 quiz' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/classroom/inputs/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/classroom").await;
    let body = list.text();
    assert!(!body.contains("Week 10 quiz"));
}
