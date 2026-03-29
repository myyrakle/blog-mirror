use std::sync::Arc;

use tracing::info;

use crate::{context::AppContext, db::run_migrations, error::Result, scheduler::sync_job};

/// One-shot: fetch new posts from Naver and store in DB, then exit.
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");
    sync_job::run(ctx).await
}
