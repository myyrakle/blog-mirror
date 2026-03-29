use std::sync::Arc;

use tracing::info;

use crate::{context::AppContext, db::run_migrations, error::Result, scheduler::replicate_job};

/// One-shot: replicate posts from DB to GitHub blog, then exit.
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");
    replicate_job::run(ctx).await
}
