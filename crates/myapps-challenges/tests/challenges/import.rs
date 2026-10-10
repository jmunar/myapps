use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{app, current, login};
use myapps_challenges::bundle::{self, Problem};
use myapps_challenges::dataset::Dataset;
use myapps_challenges::ops;
use myapps_challenges::services::import;
use rand::SeedableRng;
use rand::rngs::StdRng;
use sqlx::SqlitePool;

/// A scratch directory for bundle files, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "challenges-bundles-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn problem(key: &str, subject: &str, difficulty: i64, text: &str) -> Problem {
    Problem {
        source_key: key.into(),
        subject: subject.into(),
        topic: None,
        difficulty,
        source_level: format!("L{difficulty}"),
        problem: text.into(),
        solution: "the solution".into(),
        answer: "x".into(),
        answer_type: None,
        unit: None,
    }
}

async fn write(path: &Path, dataset: Dataset, problems: &[Problem]) {
    let writer = bundle::Writer::create(path, dataset, "https://example.org/ds", "MIT", "test")
        .await
        .unwrap();
    writer.insert(problems).await.unwrap();
    writer.finish().await.unwrap();
}

async fn load(pool: &SqlitePool, path: &Path) -> anyhow::Result<()> {
    import::run(pool, path.to_str().unwrap()).await
}

/// (source_key, id, problem, retired) for every problem in `dataset`.
async fn rows(pool: &SqlitePool, dataset: &str) -> Vec<(String, i64, String, bool)> {
    sqlx::query_as(
        "SELECT source_key, id, problem, retired_at IS NOT NULL FROM challenges_problems
         WHERE dataset = ? ORDER BY source_key",
    )
    .bind(dataset)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_bundle_is_the_whole_dataset_and_ids_survive_reloads() {
    let app = app().await;
    let scratch = Scratch::new();
    let first = scratch.path("first.sqlite");
    let second = scratch.path("second.sqlite");
    write(
        &first,
        Dataset::Ugphysics,
        &[
            problem("a", "Optics", 1, "A"),
            problem("b", "Optics", 2, "B"),
            problem("c", "Relativity", 1, "C"),
        ],
    )
    .await;
    write(
        &second,
        Dataset::Ugphysics,
        &[
            problem("a", "Optics", 1, "A, reworded"),
            problem("b", "Optics", 2, "B"),
            problem("d", "Optics", 3, "D"),
        ],
    )
    .await;

    load(&app.pool, &first).await.unwrap();
    let before = rows(&app.pool, "ugphysics").await;
    assert_eq!(before.len(), 3);
    let (url, license): (String, String) = sqlx::query_as(
        "SELECT source_url, license FROM challenges_problems WHERE source_key = 'a'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(
        (url.as_str(), license.as_str()),
        ("https://example.org/ds", "MIT")
    );

    load(&app.pool, &second).await.unwrap();
    let after = rows(&app.pool, "ugphysics").await;
    let summary: Vec<_> = after
        .iter()
        .map(|(k, _, text, retired)| (k.as_str(), text.as_str(), *retired))
        .collect();
    assert_eq!(
        summary,
        [
            ("a", "A, reworded", false),
            ("b", "B", false),
            ("c", "C", true),
            ("d", "D", false),
        ]
    );
    // Updated in place, so the attempt history keeps pointing at it.
    assert_eq!(after[0].1, before[0].1);

    // A retired problem is out of every draw and count.
    assert_eq!(
        ops::subjects(&app.pool, Dataset::Ugphysics).await.unwrap(),
        ["Optics"]
    );
    let user_id = login(&app).await;
    let stats = ops::dataset_stats(&app.pool, user_id, Dataset::Ugphysics)
        .await
        .unwrap();
    assert_eq!(stats.problems, 3);

    // Back in a later bundle, it comes back under the same id.
    load(&app.pool, &first).await.unwrap();
    let again = rows(&app.pool, "ugphysics").await;
    assert_eq!((again[2].1, again[2].3), (before[2].1, false));
    assert!(again[3].3, "d is not in the first bundle");

    let (problems, prepared_by): (i64, String) = sqlx::query_as(
        "SELECT problems, prepared_by FROM challenges_imports WHERE dataset = 'ugphysics'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!((problems, prepared_by.as_str()), (3, "test"));
}

#[tokio::test]
async fn a_retired_current_problem_is_replaced_by_a_fresh_draw() {
    let app = app().await;
    let user_id = login(&app).await;
    let scratch = Scratch::new();
    let both = scratch.path("both.sqlite");
    let only_b = scratch.path("only-b.sqlite");
    write(
        &both,
        Dataset::HendrycksMath,
        &[
            problem("a", "Algebra", 1, "A"),
            problem("b", "Algebra", 1, "B"),
        ],
    )
    .await;
    write(
        &only_b,
        Dataset::HendrycksMath,
        &[problem("b", "Algebra", 1, "B")],
    )
    .await;
    load(&app.pool, &both).await.unwrap();
    let ids = rows(&app.pool, "hendrycks-math").await;
    let (a, b) = (ids[0].1, ids[1].1);

    sqlx::query("INSERT INTO challenges_current (user_id, dataset, problem_id) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind("hendrycks-math")
        .bind(a)
        .execute(&app.pool)
        .await
        .unwrap();
    load(&app.pool, &only_b).await.unwrap();

    let mut rng = StdRng::seed_from_u64(0);
    let drawn = ops::current_or_draw(&app.pool, user_id, Dataset::HendrycksMath, &mut rng)
        .await
        .unwrap();
    assert_eq!(drawn, Some(b));
    assert_eq!(current(&app.pool, user_id, "hendrycks-math").await, Some(b));
}

#[tokio::test]
async fn a_bad_bundle_changes_nothing() {
    let app = app().await;
    let scratch = Scratch::new();
    let good = scratch.path("good.sqlite");
    write(&good, Dataset::Ugphysics, &[problem("a", "Optics", 1, "A")]).await;
    load(&app.pool, &good).await.unwrap();

    // Valid rows ahead of the bad one must not land either.
    let out_of_range = scratch.path("out-of-range.sqlite");
    write(
        &out_of_range,
        Dataset::Ugphysics,
        &[
            problem("a", "Optics", 1, "A, changed"),
            problem("b", "Optics", 1, "B"),
            problem("c", "Optics", 4, "C"),
        ],
    )
    .await;
    let err = load(&app.pool, &out_of_range).await.unwrap_err();
    assert!(format!("{err:#}").contains("difficulty 4"), "{err:#}");

    let empty_subject = scratch.path("empty-subject.sqlite");
    write(
        &empty_subject,
        Dataset::Ugphysics,
        &[problem("a", " ", 1, "A")],
    )
    .await;
    let err = load(&app.pool, &empty_subject).await.unwrap_err();
    assert!(format!("{err:#}").contains("empty subject"), "{err:#}");

    // An empty bundle would retire the whole dataset.
    let empty = scratch.path("empty.sqlite");
    write(&empty, Dataset::Ugphysics, &[]).await;
    let err = load(&app.pool, &empty).await.unwrap_err();
    assert!(format!("{err:#}").contains("no problems"), "{err:#}");

    let newer = scratch.path("newer.sqlite");
    write(
        &newer,
        Dataset::Ugphysics,
        &[problem("b", "Optics", 1, "B")],
    )
    .await;
    let file = sqlx::SqlitePool::connect(&format!("sqlite:{}", newer.display()))
        .await
        .unwrap();
    sqlx::query("UPDATE manifest SET format = format + 1")
        .execute(&file)
        .await
        .unwrap();
    file.close().await;
    let err = load(&app.pool, &newer).await.unwrap_err();
    assert!(format!("{err:#}").contains("bundle format"), "{err:#}");

    assert!(
        load(&app.pool, &scratch.path("missing.sqlite"))
            .await
            .is_err()
    );

    let summary: Vec<_> = rows(&app.pool, "ugphysics")
        .await
        .into_iter()
        .map(|(k, _, text, retired)| (k, text, retired))
        .collect();
    assert_eq!(summary, [("a".to_string(), "A".to_string(), false)]);
}

#[tokio::test]
async fn an_interrupted_preparation_leaves_no_bundle() {
    let scratch = Scratch::new();
    let path = scratch.path("ds.sqlite");
    let writer = bundle::Writer::create(&path, Dataset::Ugphysics, "u", "MIT", "test")
        .await
        .unwrap();
    writer
        .insert(&[problem("a", "Optics", 1, "A")])
        .await
        .unwrap();
    drop(writer);
    assert!(!path.exists());
}

#[tokio::test]
async fn a_real_load_retires_the_seed_samples() {
    let app = app().await;
    app.seed_and_login(&myapps_challenges::ChallengesApp).await;
    let scratch = Scratch::new();
    let path = scratch.path("math.sqlite");
    write(
        &path,
        Dataset::HendrycksMath,
        &[problem("x", "Geometry", 2, "X")],
    )
    .await;
    load(&app.pool, &path).await.unwrap();

    assert_eq!(
        ops::subjects(&app.pool, Dataset::HendrycksMath)
            .await
            .unwrap(),
        ["Geometry"]
    );
    // The other dataset's samples are untouched.
    assert!(
        !ops::subjects(&app.pool, Dataset::Ugphysics)
            .await
            .unwrap()
            .is_empty()
    );
}
