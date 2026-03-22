use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    myapps_core::cli::init();
    let cli = myapps_core::cli::Cli::parse();
    let apps = myapps::all_app_instances();
    myapps_core::cli::run(cli, apps).await
}
