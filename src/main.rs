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
    Seed {
        /// Which app to seed (currently only "leanfin")
        #[arg(long, default_value = "leanfin")]
        app: String,
        /// Wipe existing demo data before re-seeding
        #[arg(long)]
        reset: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env from the binary's directory (e.g. /opt/myapps/.env),
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
                .unwrap_or_else(|_| "myapps=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();
    let config = config::Config::from_env()?;
    let pool = db::init(&config.database_url).await?;

    match cli.command {
        Command::Serve => routes::serve(pool, config).await?,
        Command::Sync => apps::leanfin::services::sync::run(&pool, &config).await?,
        Command::CreateUser { username, password } => {
            auth::create_user(&pool, &username, &password).await?;
            tracing::info!("User '{username}' created");
        }
        Command::Seed { app, reset } => match app.as_str() {
            "leanfin" => apps::leanfin::services::seed::run(&pool, reset).await?,
            "mindflow" => apps::mindflow::services::seed::run(&pool, reset).await?,
            other => anyhow::bail!("Unknown app: {other}. Available: leanfin, mindflow"),
        },
    }

    Ok(())
}
