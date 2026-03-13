use axum_test::TestServer;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
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

    sqlx::migrate!().run(&pool).await.unwrap();

    let config = myapps::config::Config {
        database_url: db_url,
        base_url: None,
        enable_banking_app_id: None,
        enable_banking_key_path: None,
        telegram_bot_token: None,
        telegram_chat_id: None,
        bind_addr: "127.0.0.1:0".into(),
        base_path: String::new(),
    };

    let app = myapps::routes::build_router(pool.clone(), config);

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

    /// Seed full LeanFin demo data and log in as the demo user.
    pub async fn seed_and_login(&self) {
        myapps::apps::leanfin::services::seed::run(&self.pool)
            .await
            .unwrap();

        self.server
            .post("/login")
            .form(&serde_json::json!({
                "username": "demo",
                "password": "demo",
            }))
            .expect_failure() // login redirects with 303
            .await;
    }
}
