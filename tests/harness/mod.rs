use axum_test::TestServer;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TestApp {
    pub server: TestServer,
    pub pool: SqlitePool,
}

/// Spin up a fresh app instance with an in-memory SQLite database.
/// Each call gets an isolated database, so tests don't interfere.
pub async fn spawn_app() -> TestApp {
    spawn_app_with_deploy_apps(None).await
}

/// Spin up a fresh app instance limited to a subset of apps.
pub async fn spawn_app_with_deploy_apps(deploy_apps: Option<Vec<String>>) -> TestApp {
    let db_id = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_url = format!("sqlite:file:test_{db_id}?mode=memory&cache=shared");

    let options = SqliteConnectOptions::from_str(&db_url)
        .unwrap()
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .unwrap();

    let all_apps = myapps::all_app_instances();
    let migrators: Vec<_> = all_apps.iter().map(|a| a.migrations()).collect();
    myapps::db::migrator(&migrators).run(&pool).await.unwrap();

    let config = myapps::config::Config {
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
        deploy_apps,
        llama_server_url: String::new(),
        seed: false,
        cleanup_inactive_days: 0,
        static_version: String::new(),
    };

    let apps = myapps::deployed_app_instances(&config);
    let app = myapps::routes::build_router(pool.clone(), config, apps);

    let server = TestServer::builder()
        .save_cookies()
        .expect_success_by_default()
        .build(app);

    TestApp { server, pool }
}

impl TestApp {
    /// Create a user and log in. The session cookie is saved automatically.
    pub async fn login_as(&self, username: &str, password: &str) {
        myapps::auth::create_user(&self.pool, username, password)
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

    /// Seed full LeanFin demo data and log in as the seed user.
    pub async fn seed_and_login(&self) {
        let user_id = myapps::auth::create_user(&self.pool, "seeduser", "seeduser")
            .await
            .unwrap();
        let app = myapps::apps::leanfin::LeanFinApp;
        myapps::apps::leanfin::services::seed::run(&self.pool, user_id, &app)
            .await
            .unwrap();

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": "seeduser",
                "password": "seeduser",
            }))
            .expect_failure() // login redirects with 303
            .await;
    }

    /// Seed full MindFlow demo data and log in as the seed user.
    pub async fn seed_and_login_mindflow(&self) {
        let user_id = myapps::auth::create_user(&self.pool, "seeduser", "seeduser")
            .await
            .unwrap();
        let app = myapps::apps::mindflow::MindFlowApp;
        myapps::apps::mindflow::services::seed::run(&self.pool, user_id, &app)
            .await
            .unwrap();

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": "seeduser",
                "password": "seeduser",
            }))
            .expect_failure()
            .await;
    }

    /// Seed full ClassroomInput demo data and log in as the seed user.
    pub async fn seed_and_login_classroom(&self) {
        let user_id = myapps::auth::create_user(&self.pool, "seeduser", "seeduser")
            .await
            .unwrap();
        let app = myapps::apps::classroom_input::ClassroomInputApp;
        myapps::apps::classroom_input::services::seed::run(&self.pool, user_id, &app)
            .await
            .unwrap();

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": "seeduser",
                "password": "seeduser",
            }))
            .expect_failure()
            .await;
    }
}
