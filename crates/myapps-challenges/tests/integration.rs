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

/// The problem in progress in `dataset`, straight from the table.
pub async fn current(pool: &SqlitePool, user_id: i64, dataset: &str) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT problem_id FROM challenges_current WHERE user_id = ? AND dataset = ?",
    )
    .bind(user_id)
    .bind(dataset)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// Mark `problem` in `dataset` the way the htmx form does.
pub async fn mark(
    app: &TestApp,
    dataset: &str,
    problem: i64,
    correct: bool,
) -> axum_test::TestResponse {
    app.server
        .post(&format!("/challenges/practice/{dataset}/attempt"))
        .add_header("hx-request", "true")
        .form(&serde_json::json!({ "problem": problem, "correct": u8::from(correct) }))
        .await
}
