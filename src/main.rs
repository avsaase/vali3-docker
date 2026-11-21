use std::path::PathBuf;

use crate::config::Config;

mod config;
mod error;
mod igc;
mod server;
mod utils;
mod validate;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let result = run().await;

    if let Err(error) = &result {
        tracing::error!(%error, "fatal error");
    }

    result
}

async fn run() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let modules = config.load_modules()?;

    match std::env::args().nth(1) {
        Some(path) => server::validate_file(config, modules, PathBuf::from(path)).await,
        None => server::serve(config, modules).await,
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
