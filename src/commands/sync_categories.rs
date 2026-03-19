use std::sync::Arc;

use tracing::info;

use crate::{
    context::AppContext,
    crawler::NaverCrawler,
    db::category_repo::{CategoryRepo, UpsertCategory},
    error::Result,
};

/// One-shot category sync: fetches all categories from Naver and upserts them into DB.
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("sync-categories: starting");

    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());
    let category_repo = CategoryRepo::new(ctx.pool.clone());

    let cats = crawler.fetch_categories().await?;
    let count = cats.len();

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

    info!(count, "sync-categories: upserted all categories");
    Ok(())
}
