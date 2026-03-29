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
    /// Run initial full sync of all Naver blog posts to database
    Init,
    /// One-shot: fetch new posts from Naver and store in DB
    Fetch,
    /// One-shot: replicate posts from DB to GitHub blog
    Publish,
    /// One-shot: fetch categories from Naver and upsert into DB
    SyncCategories,
    /// Infinite loop: runs fetch + publish on a fixed interval (default 3600s)
    SyncLoop {
        /// Interval between runs in seconds (default: 3600)
        #[arg(long, default_value_t = 3600)]
        interval: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

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
        Commands::Fetch => commands::fetch::run(ctx).await?,
        Commands::Publish => commands::publish::run(ctx).await?,
        Commands::SyncCategories => commands::sync_categories::run(ctx).await?,
        Commands::SyncLoop { interval } => commands::sync_loop::run(ctx, interval).await?,
    }

    Ok(())
}
