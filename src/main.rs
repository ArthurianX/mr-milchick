mod app;
mod cli;
mod error;

mod actions;
mod comment;
mod config;
mod context;
mod domain;
mod gitlab;
mod notifications;
mod output;
mod rules;
mod tone;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

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
