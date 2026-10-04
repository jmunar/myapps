use crate::{app, draw, insert_problem, login, problem_id};
use myapps_challenges::ChallengesApp;

#[tokio::test]
async fn picker_requires_authentication() {
    let app = app().await;
    let response = app.server.get("/challenges").expect_failure().await;
    assert_eq!(response.status_code(), 303);
}

#[tokio::test]
async fn picker_says_how_to_import_an_empty_dataset() {
    let app = app().await;
    login(&app).await;
    let body = app.server.get("/challenges").await.text();
    assert!(body.contains("UGPhysics"));
    assert!(body.contains("Hendrycks MATH"));
    assert!(body.contains("myapps import --app challenges --dataset ugphysics"));
    assert!(body.contains("myapps import --app challenges --dataset hendrycks-math"));
    assert!(!body.contains(r#"action="/challenges/draw""#));
}

#[tokio::test]
async fn seeded_picker_offers_both_datasets() {
    let app = app().await;
    app.seed_and_login(&ChallengesApp).await;
    let body = app.server.get("/challenges").await.text();
    assert!(!body.contains("myapps import"));
    assert!(body.contains(r#"name="dataset" value="ugphysics""#));
    assert!(body.contains(r#"name="dataset" value="hendrycks-math""#));
    assert!(body.contains("6 problems · 3 subjects"));
    // The seed's history: 2 of 3 right.
    assert!(body.contains("67%"));
}

#[tokio::test]
async fn problem_page_renders_problem_solution_and_marking_form() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "ugphysics", "Relativity", 2, "Find $\\gamma$.").await;

    let body = app
        .server
        .get(&format!("/challenges/problems/{id}"))
        .await
        .text();
    assert!(body.contains("Relativity"));
    assert!(body.contains("Find $\\gamma$."));
    assert!(body.contains("the solution"));
    assert!(body.contains("Level 2 of 3 (Laws Application)"));
    assert!(body.contains(&format!(r#"action="/challenges/problems/{id}/attempt""#)));
    assert!(body.contains(r#"name="correct" value="1""#));
    assert!(body.contains(r#"name="correct" value="0""#));
    // Skipping excludes the current problem from the next draw.
    assert!(body.contains(&format!(r#"name="exclude" value="{id}""#)));
    assert!(body.contains("<details class=\"card challenges-reveal\">"));
}

/// `challenges-math.js` finds its elements by this attribute and nothing
/// type-checks the pairing: a rename on either side breaks only in the browser.
#[tokio::test]
async fn problem_page_renders_what_the_math_script_looks_for() {
    let script = include_str!("../../static/challenges-math.js");
    assert!(script.contains("[data-challenges-math]"));
    assert!(script.contains("renderMathInElement"));

    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "1+1").await;
    let body = app
        .server
        .get(&format!("/challenges/problems/{id}"))
        .await
        .text();
    assert!(body.contains("data-challenges-math>"));
    assert!(body.contains("/static/katex/katex.min.js"));
    assert!(body.contains("/static/katex/auto-render.min.js"));
    assert!(body.contains("/static/katex/katex.min.css"));
}

#[tokio::test]
async fn problem_text_is_escaped() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(
        &app.pool,
        "ugphysics",
        "<b>Optics</b>",
        1,
        "</script><img src=x onerror=alert(1)>",
    )
    .await;
    let body = app
        .server
        .get(&format!("/challenges/problems/{id}"))
        .await
        .text();
    assert!(!body.contains("<img src=x"));
    assert!(body.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!body.contains("<b>Optics</b>"));
    assert!(body.contains("<title>Challenges — &lt;b&gt;Optics&lt;/b&gt;</title>"));
}

#[tokio::test]
async fn unknown_problem_is_404() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .get("/challenges/problems/999")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 404);
    assert!(response.text().contains("Problem not found."));
}

#[tokio::test]
async fn first_right_answer_levels_up_and_says_so() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 2, "p2").await;

    let body = app
        .server
        .post(&format!("/challenges/problems/{id}/attempt"))
        .form(&serde_json::json!({ "correct": "1" }))
        .await
        .text();
    assert!(body.contains("Level up"));
    assert!(body.contains("Relativity: Level 1 → 2 of 3"));
    assert!(body.contains(r#"name="dataset" value="ugphysics""#));
}

#[tokio::test]
async fn wrong_answer_at_the_bottom_goes_straight_to_the_next_problem() {
    let app = app().await;
    login(&app).await;
    let first = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    let second = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p2").await;

    let response = app
        .server
        .post(&format!("/challenges/problems/{first}/attempt"))
        .form(&serde_json::json!({ "correct": "0" }))
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(problem_id(&response), second);
}

#[tokio::test]
async fn a_double_submit_is_recorded_once() {
    let app = app().await;
    login(&app).await;
    let id = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "p1").await;
    insert_problem(&app.pool, "ugphysics", "Relativity", 2, "p2").await;

    // The first levels up and says so; the second changes nothing and moves on.
    let first = app
        .server
        .post(&format!("/challenges/problems/{id}/attempt"))
        .form(&serde_json::json!({ "correct": "1" }))
        .await;
    assert!(first.text().contains("Level up"));
    let second = app
        .server
        .post(&format!("/challenges/problems/{id}/attempt"))
        .form(&serde_json::json!({ "correct": "1" }))
        .expect_failure()
        .await;
    assert_eq!(second.status_code(), 303);
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_attempts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
    let level: i64 = sqlx::query_scalar("SELECT level FROM challenges_progress")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(level, 2);
}

#[tokio::test]
async fn draw_with_an_unknown_dataset_goes_back_to_the_picker() {
    let app = app().await;
    login(&app).await;
    let response = draw(&app, "u-math").await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges");
}
