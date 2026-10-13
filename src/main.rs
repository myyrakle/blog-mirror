mod commands;
mod config;
mod context;
mod converter;
mod crawler;
mod db;
mod error;
mod github;
mod scheduler;
mod web;

use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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
    /// One-shot: backfill missing publish times from Naver detail pages
    BackfillDates {
        /// Maximum number of posts to process in this run
        #[arg(long, default_value_t = 500)]
        limit: i64,
    },
    /// Run the admin web dashboard (optionally with the periodic sync built in)
    Serve {
        /// Port to listen on (defaults to WEB_PORT, or 8080)
        #[arg(long)]
        port: Option<u16>,
        /// Also run fetch + publish on this interval, in seconds. 0 disables it.
        #[arg(long, default_value_t = 3600)]
        interval: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // `JobLogLayer` tees job output into the dashboard's live log view.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(web::jobs::JobLogLayer)
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
        Commands::SyncCategories => {
            commands::sync_categories::run(ctx).await?;
        }
        Commands::SyncLoop { interval } => commands::sync_loop::run(ctx, interval).await?,
        Commands::BackfillDates { limit } => {
            db::run_migrations(&ctx.pool).await?;
            let r = commands::backfill_dates::run(ctx, limit).await?;
            info!(
                examined = r.examined,
                updated = r.updated,
                no_date = r.no_date,
                failed = r.failed,
                "backfill-dates: done"
            );
        }
        Commands::Serve { port, interval } => {
            let port = port.unwrap_or(ctx.config.web_port);
            web::serve(ctx, port, interval).await?
        }
    }

    Ok(())
}
