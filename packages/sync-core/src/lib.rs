mod archive;
mod cli;
mod config;
mod engine;
mod manifest;
mod model;
mod operation_lock;
mod server;
mod state;

use anyhow::Result;

pub async fn run(program: &str) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lan_save_sync=info,tower_http=info".into()),
        )
        .with_target(false)
        .init();
    cli::execute(program).await
}
