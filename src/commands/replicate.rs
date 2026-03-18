use std::sync::Arc;

use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::{
    context::AppContext,
    db::run_migrations,
    error::Result,
    github::GitRepo,
    scheduler::replicate_job,
};

/// Starts the replication daemon (DB → GitHub only):
/// 1. Run DB migrations
/// 2. Open/clone GitHub repo
/// 3. Run replication job once immediately
/// 4. Schedule replication job every 30 minutes
/// 5. Wait for Ctrl+C
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("Starting replicate daemon");

    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    let _git_repo = GitRepo::open_or_clone(ctx.config.clone())?;
    info!("GitHub repo ready at {:?}", ctx.config.github_repo_path);

    info!("Running initial replication job...");
    replicate_job::run(ctx.clone()).await?;

    let mut scheduler = JobScheduler::new().await?;

    {
        let ctx = ctx.clone();
        scheduler
            .add(Job::new_async("0 0/30 * * * *", move |_, _| {
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
    info!("Replicate scheduler started. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl_c");
    info!("Shutting down replicate daemon");
    scheduler.shutdown().await?;

    Ok(())
}
