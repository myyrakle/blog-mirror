mod commands;
mod config;
mod context;
mod converter;
mod crawler;
mod db;
mod error;
mod github;
mod scheduler;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::info;

use crate::{config::AppConfig, context::AppContext, db::create_pool};

#[derive(Parser)]
#[command(name = "blog-mirror", about = "Naver Blog → GitHub Blog mirror tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run initial full sync of Naver blog to database
    Init,
    /// Start sync daemon: crawls Naver blog and stores posts in DB (hourly)
    Sync,
    /// Start replication daemon: copies DB posts to GitHub blog (every 30 min)
    Replicate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    info!("blog-mirror starting up");

    let config = Arc::new(AppConfig::load()?);
    let pool = create_pool(&config.database_url).await?;
    let ctx = Arc::new(AppContext::new(config, pool)?);

    match cli.command {
        Commands::Init => commands::init::run(ctx).await?,
        Commands::Sync => commands::sync::run(ctx).await?,
        Commands::Replicate => commands::replicate::run(ctx).await?,
    }

    Ok(())
}
