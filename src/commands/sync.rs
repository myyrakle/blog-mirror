use std::sync::Arc;

use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::{context::AppContext, db::run_migrations, error::Result, scheduler::sync_job};

/// Starts the sync daemon (Naver → DB only):
/// 1. Run DB migrations
/// 2. Run sync job once immediately
/// 3. Schedule hourly sync job
/// 4. Wait for Ctrl+C
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("Starting sync daemon");

    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    info!("Running initial sync job...");
    sync_job::run(ctx.clone()).await?;

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

    scheduler.start().await?;
    info!("Sync scheduler started. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl_c");
    info!("Shutting down sync daemon");
    scheduler.shutdown().await?;

    Ok(())
}
