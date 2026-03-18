use std::sync::Arc;

use tracing::info;

use crate::{
    context::AppContext,
    crawler::NaverCrawler,
    db::{
        category_repo::{CategoryRepo, UpsertCategory},
        cursor_repo::CursorRepo,
        post_repo::{PostRepo, UpsertPost},
    },
    error::Result,
};

/// Periodic sync job:
/// 1. Read current cursor
/// 2. Fetch new posts since cursor
/// 3. Upsert categories and posts
/// 4. Update cursor
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("sync_job: starting");

    let cursor_repo = CursorRepo::new(ctx.pool.clone());
    let category_repo = CategoryRepo::new(ctx.pool.clone());
    let post_repo = PostRepo::new(ctx.pool.clone());
    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());

    let cursor = cursor_repo.get_cursor(&ctx.config.naver_blog_id).await?;
    info!(cursor, "sync_job: current cursor");

    let new_posts = crawler.fetch_all_posts_until(cursor).await?;
    if new_posts.is_empty() {
        info!("sync_job: no new posts");
        return Ok(());
    }
    info!(count = new_posts.len(), "sync_job: fetched new posts");

    // Upsert new categories
    let mut seen = std::collections::HashSet::new();
    let mut cats = Vec::new();
    for p in &new_posts {
        if let Some(cat_no) = p.category_no {
            if seen.insert(cat_no) {
                cats.push(UpsertCategory {
                    blog_id: ctx.config.naver_blog_id.clone(),
                    category_no: cat_no,
                    parent_no: None,
                    name: format!("Category {}", cat_no),
                    post_count: 0,
                });
            }
        }
    }
    category_repo.upsert_many(&cats).await?;

    // Upsert posts
    let upsert_posts: Vec<UpsertPost> = new_posts
        .iter()
        .map(|p| UpsertPost {
            blog_id: ctx.config.naver_blog_id.clone(),
            log_no: p.log_no,
            title: p.title.clone(),
            category_no: p.category_no,
            add_date: p.parsed_add_date(),
        })
        .collect();
    post_repo.upsert_many(&upsert_posts).await?;

    // Update cursor to new max
    let max_log_no = new_posts.iter().map(|p| p.log_no).max().unwrap_or(cursor);
    cursor_repo
        .update_cursor(&ctx.config.naver_blog_id, max_log_no)
        .await?;
    info!(max_log_no, "sync_job: cursor updated");

    info!("sync_job: complete");
    Ok(())
}
