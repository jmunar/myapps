use crate::{app, insert_problem, login};
use myapps_challenges::ChallengesApp;
use myapps_challenges::ops;

#[tokio::test]
async fn stats_requires_authentication() {
    let app = app().await;
    let response = app.server.get("/challenges/stats").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn stats_list_every_subject_with_level_and_accuracy() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "a").await;
    let b = insert_problem(&app.pool, "ugphysics", "Relativity", 2, "b").await;
    insert_problem(&app.pool, "ugphysics", "Thermodynamics", 1, "c").await;
    ops::record_attempt(&app.pool, user_id, a, true)
        .await
        .unwrap();
    ops::record_attempt(&app.pool, user_id, b, false)
        .await
        .unwrap();

    let body = app.server.get("/challenges/stats").await.text();
    assert!(body.contains("UGPhysics"));
    // Hendrycks has no problems, so no section.
    assert!(!body.contains("Hendrycks MATH"));
    assert!(body.contains(r#"<td class="challenges-subject">Relativity</td>"#));
    assert!(body.contains(r#"<td class="challenges-subject">Thermodynamics</td>"#));
    // Relativity: right at 1 (fast start → 2), wrong at 2 (→ 1).
    assert!(body.contains(r#"<td class="challenges-num">1/2</td>"#));
    assert!(body.contains(r#"<td class="challenges-acc">50%</td>"#));
    assert!(body.contains(r#"<td class="challenges-num">0/0</td>"#));
    assert!(body.contains("Total: <strong>50%</strong> (1/2)"));
    assert!(body.contains("table-cards"));
}

#[tokio::test]
async fn stats_without_any_problems_shows_the_empty_state() {
    let app = app().await;
    login(&app).await;
    let body = app.server.get("/challenges/stats").await.text();
    assert!(body.contains("No attempts yet"));
}

#[tokio::test]
async fn seeded_stats_render() {
    let app = app().await;
    app.seed_and_login(&ChallengesApp).await;
    let body = app.server.get("/challenges/stats").await.text();
    assert!(body.contains("Classical Mechanics"));
    assert!(body.contains("Hendrycks MATH"));
}
