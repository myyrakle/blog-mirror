use std::sync::Arc;

use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::{
    context::AppContext,
    db::run_migrations,
    error::Result,
    github::GitRepo,
    scheduler::{replicate_job, sync_job},
};

/// Starts the daemon:
/// 1. Run DB migrations
/// 2. Open/clone GitHub repo
/// 3. Run both jobs once immediately
/// 4. Schedule hourly sync and replication jobs
/// 5. Wait for Ctrl+C
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("Starting serve daemon");

    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    // Ensure the GitHub repo is present
    let _git_repo = GitRepo::open_or_clone(ctx.config.clone())?;
    info!("GitHub repo ready at {:?}", ctx.config.github_repo_path);

    // Run jobs once on startup
    info!("Running initial sync job...");
    sync_job::run(ctx.clone()).await?;

    info!("Running initial replication job...");
    replicate_job::run(ctx.clone()).await?;

    // Schedule periodic jobs
    let mut scheduler = JobScheduler::new().await?;

    {
        let ctx = ctx.clone();
        scheduler
            .add(Job::new_async("0 0 * * * *", move |_, _| {
                let ctx = ctx.clone();
                Box::pin(async move {
                    if let Err(e) = sync_job::run(ctx).await {
                        tracing::error!(error = %e, "sync_job failed");
                    }
                })
            })?)
            .await?;
    }

    {
        let ctx = ctx.clone();
        scheduler
            .add(Job::new_async("0 30 * * * *", move |_, _| {
                let ctx = ctx.clone();
                Box::pin(async move {
                    if let Err(e) = replicate_job::run(ctx).await {
                        tracing::error!(error = %e, "replicate_job failed");
                    }
                })
            })?)
            .await?;
    }

    scheduler.start().await?;
    info!("Scheduler started. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl_c");
    info!("Shutting down");
    scheduler.shutdown().await?;

    Ok(())
}
