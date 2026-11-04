use aegis_theme_dev::{run, DevArgs};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "aegis-theme-dev",
    about = "Lightweight local development server for AegisCMS themes",
    version
)]
struct Cli {
    #[command(flatten)]
    args: DevArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,aegis_theme_dev=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();
    run(cli.args).await
}
