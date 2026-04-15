use axum_test::TestServer;
use myapps_core::registry::App;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TestApp {
    pub server: TestServer,
    pub pool: SqlitePool,
}

/// Spin up a fresh app instance with the given apps and an in-memory SQLite database.
/// Each call gets an isolated database, so tests don't interfere.
pub async fn spawn_app(apps: Vec<Box<dyn App>>) -> TestApp {
    let db_id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_url = format!("sqlite:file:test_{db_id}?mode=memory&cache=shared");

    let options = SqliteConnectOptions::from_str(&db_url)
        .unwrap()
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .unwrap();

    let migrators: Vec<_> = apps.iter().map(|a| a.migrations()).collect();
    myapps_core::db::migrator(&migrators)
        .run(&pool)
        .await
        .unwrap();

    // Create per-app scoped pools for database isolation
    let all_keys: Vec<&'static str> = apps.iter().map(|a| a.info().key).collect();
    let mut app_pools: HashMap<&'static str, SqlitePool> = HashMap::new();
    for app in &apps {
        let key = app.info().key;
        let others: Vec<&str> = all_keys.iter().copied().filter(|k| *k != key).collect();
        let scoped = myapps_core::db::init_scoped(&db_url, key, &others)
            .await
            .unwrap();
        app_pools.insert(key, scoped);
    }

    let config = myapps_core::config::Config {
        database_url: db_url,
        base_url: None,
        encryption_key: None,
        vapid_private_key: None,
        vapid_public_key: None,
        vapid_subject: None,
        bind_addr: "127.0.0.1:0".into(),
        base_path: String::new(),
        whisper_cli_path: "whisper-cli".into(),
        whisper_models_dir: "models".into(),
        deploy_apps: None,
        llama_server_url: String::new(),
        seed: false,
        cleanup_inactive_days: 0,
        static_version: String::new(),
        external_apps: Vec::new(),
        version: String::new(),
        build_timestamp: String::new(),
    };

    let app = myapps_core::routes::build_router(pool.clone(), app_pools, config, apps);

    let server = TestServer::builder()
        .save_cookies()
        .expect_success_by_default()
        .build(app);

    TestApp { server, pool }
}

impl TestApp {
    /// Create a user and log in. The session cookie is saved automatically.
    pub async fn login_as(&self, username: &str, password: &str) {
        myapps_core::auth::create_user(&self.pool, username, password)
            .await
            .unwrap();

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .expect_failure() // login redirects with 303
            .await;
    }

    /// Create a user, seed the given app, and log in.
    pub async fn seed_and_login(&self, app: &dyn App) {
        let user_id = myapps_core::auth::create_user(&self.pool, "seeduser", "seeduser")
            .await
            .unwrap();

        if let Some(fut) = app.seed(&self.pool, user_id) {
            fut.await.unwrap();
        }

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": "seeduser",
                "password": "seeduser",
            }))
            .expect_failure() // login redirects with 303
            .await;
    }
}
