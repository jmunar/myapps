use clap::{Parser, Subcommand};
use sqlx::SqlitePool;

use crate::config::Config;
use crate::registry::App;

#[derive(Parser)]
#[command(name = "myapps", about = "Multi-app platform")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the HTTP server
    Serve,
    /// Run scheduled tasks for all deployed apps (e.g. daily via system cron)
    Cron,
    /// Create a new user
    CreateUser {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// Generate an invite link for a new user (valid 48h, single-use)
    Invite,
    /// Generate VAPID key pair for Web Push notifications
    GenerateVapidKeys,
    /// Populate database with seed data for a given user
    Seed {
        /// Username to seed data for (must already exist)
        #[arg(long)]
        user: String,
    },
    /// Delete a user and all their data
    DeleteUser {
        #[arg(long)]
        username: String,
    },
    /// Delete all app data for a user (keeps the user account)
    DeleteUserAppData {
        #[arg(long)]
        username: String,
        /// App key to delete data for (e.g. leanfin). Omit to delete all apps.
        #[arg(long)]
        app: Option<String>,
    },
    /// Delete users who have been inactive for the given number of days
    CleanupUsers {
        /// Number of days of inactivity before a user is deleted
        #[arg(long, default_value = "7")]
        days: i64,
    },
}

/// Load `.env` files and initialise the tracing subscriber.
pub fn init() {
    // Load .env from the binary's directory (e.g. /opt/myapps/.env),
    // so commands work regardless of the caller's working directory.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let env_path = dir.join(".env");
        if env_path.exists() {
            dotenvy::from_path(&env_path).ok();
        }
    }
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "myapps=info".parse().unwrap()),
        )
        .init();
}

/// Run the CLI command with the given app instances.
pub async fn run(cli: Cli, apps: Vec<Box<dyn App>>) -> anyhow::Result<()> {
    let mut config = Config::from_env()?;
    let migrators: Vec<_> = apps.iter().map(|a| a.migrations()).collect();
    let pool = crate::db::init(&config.database_url, &migrators).await?;

    let deployed: Vec<Box<dyn App>> = crate::registry::deployed_app_instances(apps, &config);

    // Recompute static version including app CSS for cache busting
    let app_css: Vec<&str> = deployed.iter().map(|a| a.css()).collect();
    config.static_version = Config::compute_static_version_with_extra(&app_css);

    match cli.command {
        Command::Serve => {
            crate::routes::serve(pool, config, deployed).await?;
        }
        Command::Cron => {
            run_cron(&pool, &config, &deployed).await;
        }
        Command::CreateUser { username, password } => {
            crate::auth::create_user(&pool, &username, &password).await?;
            tracing::info!("User '{username}' created");
        }
        Command::Invite => {
            let token = crate::auth::create_invite(&pool).await?;
            let fallback = format!("http://{}", config.bind_addr);
            let base = config.base_url.as_deref().unwrap_or(&fallback);
            println!("Invite link (valid 48h, single-use):\n{base}/invite/{token}");
        }
        Command::GenerateVapidKeys => {
            generate_vapid_keys();
        }
        Command::Seed { user } => {
            run_seed(&pool, &deployed, &user).await?;
        }
        Command::DeleteUser { username } => {
            let result = sqlx::query("DELETE FROM users WHERE username = ?")
                .bind(&username)
                .execute(&pool)
                .await?;
            anyhow::ensure!(result.rows_affected() > 0, "User '{username}' not found");
            tracing::info!("Deleted user '{username}' and all their data");
        }
        Command::DeleteUserAppData { username, app } => {
            run_delete_app_data(&pool, &deployed, &username, app.as_deref()).await?;
        }
        Command::CleanupUsers { days } => {
            let count = crate::auth::cleanup_inactive_users(&pool, days).await?;
            tracing::info!("Deleted {count} inactive users (>{days} days)");
        }
    }

    Ok(())
}

async fn run_cron(pool: &SqlitePool, config: &Config, apps: &[Box<dyn App>]) {
    for app in apps {
        if let Some(fut) = app.cron(pool, config) {
            let key = app.info().key;
            tracing::info!("Running cron for {key}");
            if let Err(e) = fut.await {
                tracing::error!("Cron failed for {key}: {e}");
            }
        }
    }
}

async fn run_seed(pool: &SqlitePool, apps: &[Box<dyn App>], user: &str) -> anyhow::Result<()> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(user)
        .fetch_optional(pool)
        .await?;
    let user_id = row
        .map(|r| r.0)
        .ok_or_else(|| anyhow::anyhow!("User '{user}' not found"))?;

    for app in apps {
        if let Some(fut) = app.seed(pool, user_id) {
            fut.await?;
        }
    }
    tracing::info!("Seed complete for user '{user}'");
    Ok(())
}

async fn run_delete_app_data(
    pool: &SqlitePool,
    apps: &[Box<dyn App>],
    username: &str,
    app_key: Option<&str>,
) -> anyhow::Result<()> {
    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("User '{username}' not found"))?;

    let targets: Vec<&dyn App> = match app_key {
        Some(key) => {
            let found = apps.iter().find(|a| a.info().key == key);
            match found {
                Some(a) => vec![a.as_ref()],
                None => anyhow::bail!("Unknown app '{key}'"),
            }
        }
        None => apps.iter().map(|a| a.as_ref()).collect(),
    };

    for a in &targets {
        crate::registry::delete_user_app_data(pool, *a, user_id).await?;
    }
    let names: Vec<&str> = targets.iter().map(|a| a.info().key).collect();
    tracing::info!(
        "Deleted app data for user '{username}': {}",
        names.join(", ")
    );
    Ok(())
}

fn generate_vapid_keys() {
    use base64::Engine;
    use web_push::VapidSignatureBuilder;

    let private_bytes: [u8; 32] = rand::random();
    let private_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(private_bytes);

    let partial = VapidSignatureBuilder::from_base64_no_sub(&private_b64)
        .expect("Generated key should be valid");
    let public_bytes = partial.get_public_key();
    let public_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&public_bytes);

    println!("Add these to your .env file:\n");
    println!("VAPID_PRIVATE_KEY={private_b64}");
    println!("VAPID_PUBLIC_KEY={public_b64}");
    println!("VAPID_SUBJECT=mailto:you@example.com");
}
