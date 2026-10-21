use std::collections::HashSet;

use crate::{app, current, insert_problem, login};
use myapps_challenges::dataset::Dataset;
use myapps_challenges::{ops, selector};
use myapps_test_harness::TestApp;
use rand::SeedableRng;
use rand::rngs::StdRng;

#[tokio::test]
async fn cold_start_draws_from_the_easiest_levels() {
    let app = app().await;
    let user_id = login(&app).await;
    let easy = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "easy").await;
    let medium = insert_problem(&app.pool, "hendrycks-math", "Algebra", 2, "medium").await;
    insert_problem(&app.pool, "hendrycks-math", "Algebra", 3, "hard").await;
    insert_problem(&app.pool, "hendrycks-math", "Algebra", 5, "hardest").await;

    let mut seen = HashSet::new();
    let mut rng = StdRng::seed_from_u64(0);
    for _ in 0..30 {
        let id = ops::draw(&app.pool, user_id, Dataset::HendrycksMath, None, &mut rng).await;
        seen.insert(id.unwrap().unwrap());
    }
    assert_eq!(seen, HashSet::from([easy, medium]));
}

#[tokio::test]
async fn unseen_problems_come_first() {
    let app = app().await;
    let user_id = login(&app).await;
    let seen = insert_problem(&app.pool, "ugphysics", "Optics", 1, "seen").await;
    let unseen = insert_problem(&app.pool, "ugphysics", "Optics", 1, "unseen").await;
    // Wrong, so the level stays at 1 and both are in the neighbourhood.
    ops::record_attempt(&app.pool, user_id, seen, false)
        .await
        .unwrap();

    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..20 {
        let id = ops::draw(&app.pool, user_id, Dataset::Ugphysics, None, &mut rng)
            .await
            .unwrap();
        assert_eq!(id, Some(unseen));
    }
}

#[tokio::test]
async fn falls_back_beyond_the_neighbourhood_then_repeats_the_oldest() {
    let app = app().await;
    let user_id = login(&app).await;
    let far = insert_problem(&app.pool, "hendrycks-math", "Algebra", 5, "far").await;
    let mut rng = StdRng::seed_from_u64(2);

    // Level 1, nothing at 1 or 2: the draw reaches level 5.
    let id = ops::draw(&app.pool, user_id, Dataset::HendrycksMath, None, &mut rng).await;
    assert_eq!(id.unwrap(), Some(far));

    // Seen everything: repeat the one seen longest ago.
    let near = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "near").await;
    ops::record_attempt(&app.pool, user_id, far, false)
        .await
        .unwrap();
    ops::record_attempt(&app.pool, user_id, near, false)
        .await
        .unwrap();
    let id = ops::draw(&app.pool, user_id, Dataset::HendrycksMath, None, &mut rng).await;
    assert_eq!(id.unwrap(), Some(far));
}

#[tokio::test]
async fn skip_excludes_the_current_problem() {
    let app = app().await;
    let user_id = login(&app).await;
    let a = insert_problem(&app.pool, "ugphysics", "Optics", 1, "a").await;
    let b = insert_problem(&app.pool, "ugphysics", "Optics", 1, "b").await;
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..20 {
        let id = ops::draw(&app.pool, user_id, Dataset::Ugphysics, Some(a), &mut rng).await;
        assert_eq!(id.unwrap(), Some(b));
    }
    // A one-problem subject still returns its problem rather than nothing.
    let mut rng = StdRng::seed_from_u64(4);
    let only = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "only").await;
    let id = ops::draw(
        &app.pool,
        user_id,
        Dataset::HendrycksMath,
        Some(only),
        &mut rng,
    )
    .await;
    assert_eq!(id.unwrap(), Some(only));
}

#[tokio::test]
async fn an_empty_dataset_draws_nothing() {
    let app = app().await;
    login(&app).await;
    let response = app
        .server
        .get("/challenges/practice/ugphysics")
        .expect_failure()
        .await;
    assert_eq!(response.status_code(), 303);
    assert_eq!(response.header("location"), "/challenges");
}

#[tokio::test]
async fn command_bar_next_problem_opens_the_practice_page() {
    let app = app().await;
    let user_id = login(&app).await;
    let id = insert_problem(&app.pool, "hendrycks-math", "Algebra", 1, "p").await;
    let params = std::collections::HashMap::from([(
        "dataset".to_string(),
        serde_json::json!("hendrycks-math"),
    )]);
    let result = ops::dispatch(&app.pool, user_id, "next_problem", &params, "")
        .await
        .unwrap();
    assert_eq!(
        result.redirect.as_deref(),
        Some("/challenges/practice/hendrycks-math")
    );
    assert_eq!(
        current(&app.pool, user_id, "hendrycks-math").await,
        Some(id)
    );

    let result = ops::dispatch(&app.pool, user_id, "next_problem", &Default::default(), "").await;
    assert_eq!(
        result.err().as_deref(),
        Some("UGPhysics has not been imported yet.")
    );
}

#[tokio::test]
async fn a_one_problem_subject_hands_over_to_the_others() {
    let app = app().await;
    let user_id = login(&app).await;
    let only = insert_problem(&app.pool, "ugphysics", "Optics", 1, "only").await;
    let other = insert_problem(&app.pool, "ugphysics", "Relativity", 1, "other").await;
    // Whichever subject the draw starts from, the excluded problem never
    // comes back while another subject has something to offer.
    for seed in 0..20 {
        let mut rng = StdRng::seed_from_u64(seed);
        let id = ops::draw(&app.pool, user_id, Dataset::Ugphysics, Some(only), &mut rng).await;
        assert_eq!(id.unwrap(), Some(other));
    }
}

/// 40 Algebra problems at Hendrycks' top level, graded 0..40, and the user
/// already there.
async fn graded_top_level(app: &TestApp, user_id: i64) -> Vec<i64> {
    let mut ids = Vec::new();
    for grade in 0..40 {
        let id = insert_problem(&app.pool, "hendrycks-math", "Algebra", 5, "top").await;
        sqlx::query(
            "INSERT INTO challenges_problem_tags (problem_id, kind, value) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(selector::GRADE_TAG)
        .bind(f64::from(grade))
        .execute(&app.pool)
        .await
        .unwrap();
        ids.push(id);
    }
    sqlx::query(
        "INSERT INTO challenges_progress (user_id, dataset, subject, level, streak, fast_start)
         VALUES (?, 'hendrycks-math', 'Algebra', 5, 0, 0)",
    )
    .bind(user_id)
    .execute(&app.pool)
    .await
    .unwrap();
    ids
}

/// The grade rank (0..40) of every problem `user_id` draws over 30 draws.
async fn graded_draws(app: &TestApp, user_id: i64, ids: &[i64]) -> HashSet<usize> {
    let mut rng = StdRng::seed_from_u64(5);
    let mut ranks = HashSet::new();
    for _ in 0..30 {
        let id = ops::draw(&app.pool, user_id, Dataset::HendrycksMath, None, &mut rng)
            .await
            .unwrap()
            .unwrap();
        ranks.insert(ids.iter().position(|&x| x == id).unwrap());
    }
    ranks
}

#[tokio::test]
async fn the_top_level_draws_by_grade_from_your_answers_there() {
    // The same four middle problems, answered right by one user and wrong by
    // the other: the first is sent above the middle, the second well below.
    let mut draws = Vec::new();
    for right in [true, false] {
        let app = app().await;
        let user_id = login(&app).await;
        let ids = graded_top_level(&app, user_id).await;
        for &id in &ids[18..22] {
            ops::record_attempt(&app.pool, user_id, id, right)
                .await
                .unwrap();
        }
        // Wrong answers drop the staircase below the top; put it back, so
        // both users draw from the same level and only the ranking differs.
        sqlx::query("UPDATE challenges_progress SET level = 5 WHERE user_id = ?")
            .bind(user_id)
            .execute(&app.pool)
            .await
            .unwrap();
        draws.push(graded_draws(&app, user_id, &ids).await);
    }
    let (strong, weak) = (&draws[0], &draws[1]);
    assert!(strong.iter().all(|&r| r >= 18), "{strong:?}");
    assert!(weak.iter().all(|&r| r < 12), "{weak:?}");
    assert!(
        strong.iter().all(|r| !(18..22).contains(r)),
        "seen problems come back"
    );
}

#[tokio::test]
async fn ungraded_top_level_problems_are_still_drawn_once_the_graded_run_out() {
    let app = app().await;
    let user_id = login(&app).await;
    let graded = graded_top_level(&app, user_id).await;
    let ungraded = insert_problem(&app.pool, "hendrycks-math", "Algebra", 5, "ungraded").await;
    for &id in &graded {
        ops::record_attempt(&app.pool, user_id, id, true)
            .await
            .unwrap();
    }
    let mut rng = StdRng::seed_from_u64(6);
    let id = ops::draw(&app.pool, user_id, Dataset::HendrycksMath, None, &mut rng).await;
    assert_eq!(id.unwrap(), Some(ungraded));
}
