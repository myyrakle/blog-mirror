use std::sync::Arc;

use tracing::info;

use crate::{
    context::AppContext,
    db::{cursor_repo::CursorRepo, run_migrations},
    error::Result,
    scheduler::sync_job::sync_pages,
};

/// Runs the initial full sync:
/// 1. Run DB migrations
/// 2. Fetch all posts page-by-page (cursor = 0), upsert each page immediately
/// 3. Update cursor to the max log_no seen
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("Starting initial sync");

    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    let cursor_repo = CursorRepo::new(ctx.pool.clone());

    // Resume from last checkpoint if a previous init was interrupted
    let cursor = cursor_repo.get_cursor(&ctx.config.naver_blog_id).await?;
    if cursor > 0 {
        info!(cursor, "Resuming from previous checkpoint");
    }

    if let Some(max_log_no) = sync_pages(ctx.clone(), cursor).await? {
        // Save the final max so future syncs only fetch newer posts
        cursor_repo
            .update_cursor(&ctx.config.naver_blog_id, max_log_no)
            .await?;
        info!(max_log_no, "Updated sync cursor");
        info!("Initial sync complete");
    } else {
        info!("No new posts found. Done.");
    }

    Ok(())
}
