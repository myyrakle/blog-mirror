use std::{collections::HashSet, sync::Arc};

use tracing::{info, warn};

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
/// 2. Fetch new posts page-by-page since cursor, upsert each page immediately
/// 3. Update cursor after all pages are processed
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("sync_job: starting");

    let cursor_repo = CursorRepo::new(ctx.pool.clone());
    let cursor = cursor_repo.get_cursor(&ctx.config.naver_blog_id).await?;
    info!(cursor, "sync_job: current cursor");

    let max_log_no = sync_pages(ctx.clone(), cursor).await?;

    if let Some(max) = max_log_no {
        cursor_repo.update_cursor(&ctx.config.naver_blog_id, max).await?;
        info!(max_log_no = max, "sync_job: cursor updated");
    } else {
        info!("sync_job: no new posts");
    }

    info!("sync_job: complete");
    Ok(())
}

/// Paginates Naver post list from page 1, stopping when log_no <= cursor.
/// Upserts each page into DB immediately and saves the cursor after each page,
/// so the process can be resumed if interrupted.
/// Returns the highest log_no seen, or None if nothing was fetched.
pub async fn sync_pages(ctx: Arc<AppContext>, cursor: i64) -> Result<Option<i64>> {
    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());
    let category_repo = CategoryRepo::new(ctx.pool.clone());
    let post_repo = PostRepo::new(ctx.pool.clone());
    let cursor_repo = CursorRepo::new(ctx.pool.clone());

    // Fetch and upsert all categories with real names first
    match crawler.fetch_categories().await {
        Ok(cats) => {
            let upsert_cats: Vec<UpsertCategory> = cats
                .into_iter()
                .map(|c| UpsertCategory {
                    blog_id: ctx.config.naver_blog_id.clone(),
                    category_no: c.category_no,
                    parent_no: c.parent_no,
                    name: c.name,
                    post_count: c.post_count,
                })
                .collect();
            category_repo.upsert_many(&upsert_cats).await?;
            info!(count = upsert_cats.len(), "Upserted categories with real names");
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch categories, will use fallback names from post list");
        }
    }

    let count_per_page = 30u32;
    let mut page = 1u32;
    let mut total = 0usize;
    let mut max_log_no: Option<i64> = None;

    loop {
        info!(page, cursor, "Fetching Naver post list page");
        let items = crawler.fetch_post_list_page(page, count_per_page).await?;
        let fetched = items.len();

        // Find items newer than cursor
        let new_items: Vec<_> = items
            .into_iter()
            .take_while(|item| item.log_no > cursor)
            .collect();

        let done = new_items.len() < fetched; // hit the cursor boundary

        if !new_items.is_empty() {
            // Insert new categories as fallback (preserves real names if already set)
            let mut seen: HashSet<i32> = HashSet::new();
            let cats: Vec<UpsertCategory> = new_items
                .iter()
                .filter_map(|p| p.category_no)
                .filter(|&n| seen.insert(n))
                .map(|cat_no| UpsertCategory {
                    blog_id: ctx.config.naver_blog_id.clone(),
                    category_no: cat_no,
                    parent_no: None,
                    name: format!("Category {}", cat_no),
                    post_count: 0,
                })
                .collect();
            category_repo.insert_many_if_not_exists(&cats).await?;

            // Upsert posts from this page
            let upsert_posts: Vec<UpsertPost> = new_items
                .iter()
                .map(|p| UpsertPost {
                    blog_id: ctx.config.naver_blog_id.clone(),
                    log_no: p.log_no,
                    title: p.title.clone(),
                    category_no: p.category_no,
                    add_date: p.parsed_add_date(),
                })
                .collect();

            // Use the minimum log_no of this page as cursor checkpoint,
            // so a restart will re-fetch this page's boundary safely.
            let page_min = new_items.iter().map(|p| p.log_no).min().unwrap_or(0);
            let page_max = new_items.iter().map(|p| p.log_no).max().unwrap_or(0);

            post_repo.upsert_many(&upsert_posts).await?;
            total += new_items.len();
            max_log_no = Some(max_log_no.unwrap_or(0).max(page_max));

            // Save checkpoint: use (page_min - 1) so a restart re-fetches
            // from just before the lowest log_no we've seen on this page.
            cursor_repo
                .update_cursor(&ctx.config.naver_blog_id, page_min - 1)
                .await?;

            info!(
                page,
                inserted = new_items.len(),
                total,
                checkpoint = page_min - 1,
                "Upserted page metadata"
            );

            // Fetch and store HTML body for each post on this page
            for item in &new_items {
                crawler.rate_limit().await;
                match crawler.fetch_post_html(item.log_no).await {
                    Ok(html) => {
                        if let Err(e) = post_repo
                            .save_body(&ctx.config.naver_blog_id, item.log_no, &html)
                            .await
                        {
                            warn!(log_no = item.log_no, error = %e, "Failed to save body");
                        } else {
                            info!(log_no = item.log_no, "Saved post body");
                        }
                    }
                    Err(e) => {
                        warn!(log_no = item.log_no, error = %e, "Failed to fetch post body");
                    }
                }
            }
        }

        if done || fetched < count_per_page as usize {
            break;
        }

        page += 1;
        crawler.rate_limit().await;
    }

    Ok(max_log_no)
}
