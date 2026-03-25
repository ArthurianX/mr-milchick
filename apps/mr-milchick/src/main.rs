use anyhow::Result;
use clap::Parser;
use mr_milchick::app;
use mr_milchick::cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    app::run(cli).await
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "mr_milchick=debug,info".to_string()),
        )
        .init();
}
