use clap::{Parser, Subcommand};
use myapps::{apps, auth, config, db, routes};

#[derive(Parser)]
#[command(name = "myapps", about = "Multi-app platform")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let cli = Cli::parse();
    let config = config::Config::from_env()?;
    let pool = db::init(&config.database_url).await?;

    match cli.command {
        Command::Serve => routes::serve(pool, config).await?,
        Command::Cron => {
            for app in apps::registry::deployed_app_instances(&config) {
                if let Some(fut) = app.cron(&pool, &config) {
                    let key = app.info().key;
                    tracing::info!("Running cron for {key}");
                    if let Err(e) = fut.await {
                        tracing::error!("Cron failed for {key}: {e}");
                    }
                }
            }
        }
        Command::CreateUser { username, password } => {
            auth::create_user(&pool, &username, &password).await?;
            tracing::info!("User '{username}' created");
        }
        Command::Invite => {
            let token = auth::create_invite(&pool).await?;
            let fallback = format!("http://{}", config.bind_addr);
            let base = config.base_url.as_deref().unwrap_or(&fallback);
            println!("Invite link (valid 48h, single-use):\n{base}/invite/{token}");
        }
        Command::GenerateVapidKeys => {
            use base64::Engine;
            use web_push::VapidSignatureBuilder;

            // Generate a random 32-byte private key for P-256
            let private_bytes: [u8; 32] = rand::random();
            let private_b64 =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(private_bytes);

            // Derive the public key via web-push
            let partial = VapidSignatureBuilder::from_base64_no_sub(&private_b64)
                .expect("Generated key should be valid");
            let public_bytes = partial.get_public_key();
            let public_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&public_bytes);

            println!("Add these to your .env file:\n");
            println!("VAPID_PRIVATE_KEY={private_b64}");
            println!("VAPID_PUBLIC_KEY={public_b64}");
            println!("VAPID_SUBJECT=mailto:you@example.com");
        }
        Command::Seed { user } => {
            let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
                .bind(&user)
                .fetch_optional(&pool)
                .await?;
            let user_id = row
                .map(|r| r.0)
                .ok_or_else(|| anyhow::anyhow!("User '{user}' not found"))?;

            for app in apps::registry::deployed_app_instances(&config) {
                if let Some(fut) = app.seed(&pool, user_id) {
                    fut.await?;
                }
            }
            tracing::info!("Seed complete for user '{user}'");
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
            let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
                .bind(&username)
                .fetch_optional(&pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("User '{username}' not found"))?;

            let all_apps = apps::registry::deployed_app_instances(&config);
            let targets: Vec<&dyn apps::registry::App> = match &app {
                Some(key) => {
                    let found = all_apps.iter().find(|a| a.info().key == key.as_str());
                    match found {
                        Some(a) => vec![a.as_ref()],
                        None => anyhow::bail!("Unknown app '{key}'"),
                    }
                }
                None => all_apps.iter().map(|a| a.as_ref()).collect(),
            };

            for a in &targets {
                apps::registry::delete_user_app_data(&pool, *a, user_id).await?;
            }
            let names: Vec<&str> = targets.iter().map(|a| a.info().key).collect();
            tracing::info!(
                "Deleted app data for user '{username}': {}",
                names.join(", ")
            );
        }
        Command::CleanupUsers { days } => {
            let count = auth::cleanup_inactive_users(&pool, days).await?;
            tracing::info!("Deleted {count} inactive users (>{days} days)");
        }
    }

    Ok(())
}
