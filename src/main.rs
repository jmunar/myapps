mod auth;
mod config;
mod db;
mod models;
mod routes;
mod services;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "leanfin", about = "Personal expense tracker")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP server
    Serve,
    /// Fetch transactions from all linked bank accounts
    Sync,
    /// Create a new user
    CreateUser {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// Populate database with demo data for local development
    Seed,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env from the binary's directory (e.g. /opt/leanfin/.env),
    // so commands work regardless of the caller's working directory.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let env_path = dir.join(".env");
            if env_path.exists() {
                dotenvy::from_path(&env_path).ok();
            }
        }
    }
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "leanfin=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let config = config::Config::from_env()?;
    let pool = db::init(&config.database_url).await?;

    match cli.command {
        Command::Serve => routes::serve(pool, config).await?,
        Command::Sync => services::sync::run(&pool, &config).await?,
        Command::CreateUser { username, password } => {
            auth::create_user(&pool, &username, &password).await?;
            tracing::info!("User '{username}' created");
        }
        Command::Seed => services::seed::run(&pool).await?,
    }

    Ok(())
}
