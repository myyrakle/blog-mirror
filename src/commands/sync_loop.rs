use std::sync::Arc;

use tokio::time::{Duration, sleep};
use tracing::info;

use crate::{
    context::AppContext,
    db::run_migrations,
    error::Result,
    scheduler::{replicate_job, sync_job},
};

/// Infinite loop: runs fetch + publish every `interval_secs` seconds.
/// Stops cleanly on Ctrl+C.
pub async fn run(ctx: Arc<AppContext>, interval_secs: u64) -> Result<()> {
    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");
    info!(interval_secs, "Starting sync-loop");

    loop {
        info!("sync-loop: running fetch");
        if let Err(e) = sync_job::run(ctx.clone()).await {
            tracing::error!(error = %e, "sync-loop: fetch failed");
        }

        info!("sync-loop: running publish");
        if let Err(e) = replicate_job::run(ctx.clone()).await {
            tracing::error!(error = %e, "sync-loop: publish failed");
        }

        info!(interval_secs, "sync-loop: sleeping");
        tokio::select! {
            _ = sleep(Duration::from_secs(interval_secs)) => {}
            _ = tokio::signal::ctrl_c() => {
                info!("sync-loop: Ctrl+C received, shutting down");
                break;
            }
        }
    }

    Ok(())
}
