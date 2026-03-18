use std::sync::Arc;

use tracing::info;

use crate::{
    context::AppContext,
    crawler::NaverCrawler,
    db::{
        category_repo::{CategoryRepo, UpsertCategory},
        cursor_repo::CursorRepo,
        post_repo::{PostRepo, UpsertPost},
        run_migrations,
    },
    error::Result,
};

/// Runs the initial full sync:
/// 1. Run DB migrations
/// 2. Fetch all posts (cursor = 0)
/// 3. Upsert categories and posts into DB
/// 4. Update cursor to the max log_no seen
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("Starting initial sync");

    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());

    // Fetch all posts from page 1 with cursor = 0 (fetch everything)
    let posts = crawler.fetch_all_posts_until(0).await?;
    info!(count = posts.len(), "Fetched posts from Naver");

    if posts.is_empty() {
        info!("No posts found. Done.");
        return Ok(());
    }

    let category_repo = CategoryRepo::new(ctx.pool.clone());
    let post_repo = PostRepo::new(ctx.pool.clone());
    let cursor_repo = CursorRepo::new(ctx.pool.clone());

    // Collect unique categories from post list
    let mut seen_categories = std::collections::HashSet::new();
    let mut categories_to_upsert = Vec::new();
    for post in &posts {
        if let Some(cat_no) = post.category_no {
            if seen_categories.insert(cat_no) {
                categories_to_upsert.push(UpsertCategory {
                    blog_id: ctx.config.naver_blog_id.clone(),
                    category_no: cat_no,
                    parent_no: None,
                    name: format!("Category {}", cat_no), // name unknown from list API
                    post_count: 0,
                });
            }
        }
    }
    category_repo.upsert_many(&categories_to_upsert).await?;
    info!(count = categories_to_upsert.len(), "Upserted categories");

    // Upsert all posts
    let upsert_posts: Vec<UpsertPost> = posts
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
    info!(count = upsert_posts.len(), "Upserted posts");

    // Update cursor to the highest log_no seen
    let max_log_no = posts.iter().map(|p| p.log_no).max().unwrap_or(0);
    cursor_repo
        .update_cursor(&ctx.config.naver_blog_id, max_log_no)
        .await?;
    info!(max_log_no, "Updated sync cursor");

    info!("Initial sync complete");
    Ok(())
}
