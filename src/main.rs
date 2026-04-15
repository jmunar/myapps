use clap::Parser;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    myapps_core::cli::init();
    let cli = myapps_core::cli::Cli::parse();
    let apps = myapps::all_app_instances();
    myapps_core::cli::run(cli, apps, VERSION, BUILD_TIMESTAMP).await
}
