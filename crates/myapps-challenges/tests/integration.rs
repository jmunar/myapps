mod challenges {
    pub mod draw;
    pub mod pages;
    pub mod stats;
}

use myapps_challenges::ChallengesApp;
use myapps_test_harness::TestApp;
use sqlx::SqlitePool;

pub async fn app() -> TestApp {
    myapps_test_harness::spawn_app(vec![Box::new(ChallengesApp)]).await
}

/// Log in as a fresh user with an empty catalogue; returns the user id.
pub async fn login(app: &TestApp) -> i64 {
    app.login_as("test", "pass").await;
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'test'")
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

pub async fn insert_problem(
    pool: &SqlitePool,
    dataset: &str,
    subject: &str,
    difficulty: i64,
    problem: &str,
) -> i64 {
    let key: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM challenges_problems")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query_scalar(
        "INSERT INTO challenges_problems
             (dataset, source_key, subject, difficulty, source_level, problem, solution,
              answer, source_url, license)
         VALUES (?, ?, ?, ?, 'lvl', ?, 'the solution', 'x', 'https://example.org', 'MIT')
         RETURNING id",
    )
    .bind(dataset)
    .bind(format!("t/{key}"))
    .bind(subject)
    .bind(difficulty)
    .bind(problem)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The problem id a 303 to `/challenges/problems/{id}` points at.
pub fn problem_id(response: &axum_test::TestResponse) -> i64 {
    let location = response.header("location");
    let location = location.to_str().unwrap();
    location
        .strip_prefix("/challenges/problems/")
        .unwrap_or_else(|| panic!("unexpected redirect: {location}"))
        .parse()
        .unwrap()
}

pub async fn draw(app: &TestApp, dataset: &str) -> axum_test::TestResponse {
    app.server
        .post("/challenges/draw")
        .form(&serde_json::json!({ "dataset": dataset }))
        .expect_failure()
        .await
}
