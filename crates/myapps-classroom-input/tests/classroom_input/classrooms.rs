#[tokio::test]
async fn classrooms_page_requires_authentication() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    let response = app
        .server
        .get("/classroom/classrooms")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn classrooms_page_renders_seeded_classrooms() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let response = app.server.get("/classroom/classrooms").await;
    let body = response.text();
    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("1-A"));
    assert!(body.contains("1-B"));
    assert!(body.contains("2-A"));
}

#[tokio::test]
async fn classrooms_page_shows_pupil_count() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let response = app.server.get("/classroom/classrooms").await;
    let body = response.text();
    // 1-A has 15 pupils
    assert!(body.contains("15 pupils"));
    // 1-B has 14 pupils
    assert!(body.contains("14 pupils"));
}

#[tokio::test]
async fn classrooms_page_has_create_form() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.login_as("test", "pass").await;

    let response = app.server.get("/classroom/classrooms").await;
    let body = response.text();
    assert!(body.contains(r#"name="label""#));
    assert!(body.contains(r#"name="pupils""#));
}

#[tokio::test]
async fn create_classroom() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.login_as("test", "pass").await;

    let response = app
        .server
        .post("/classroom/classrooms/create")
        .form(&serde_json::json!({
            "label": "3-C",
            "pupils": "Ana García\nPedro López\nMaría Torres",
        }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/classroom/classrooms").await;
    let body = list.text();
    assert!(body.contains("3-C"));
    assert!(body.contains("3 pupils"));
}

#[tokio::test]
async fn delete_classroom() {
    let app =
        myapps_test_harness::spawn_app(vec![Box::new(myapps_classroom_input::ClassroomInputApp)])
            .await;
    app.seed_and_login(&myapps_classroom_input::ClassroomInputApp)
        .await;

    let (id,): (i64,) =
        sqlx::query_as("SELECT id FROM classroom_input_classrooms WHERE label = '2-A' LIMIT 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let response = app
        .server
        .post(&format!("/classroom/classrooms/{id}/delete"))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);

    let list = app.server.get("/classroom/classrooms").await;
    let body = list.text();
    assert!(!body.contains("2-A"));
}
