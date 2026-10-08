//! One-off repair: fill in publish times that the post list API never provided.
//!
//! `PostTitleListAsync` returns `"2017. 12. 2."` with no time, so posts
//! collected from it all land on midnight and same-day posts end up in an
//! arbitrary order on the published blog. The detail page carries the real
//! timestamp, so this job walks the affected posts and reads it from there.

use std::sync::Arc;

use tracing::{info, warn};

use crate::{context::AppContext, crawler::NaverCrawler, db::post_repo::PostRepo, error::Result};

#[derive(Debug, Default)]
pub struct BackfillReport {
    pub examined: usize,
    pub updated: usize,
    /// Detail page fetched but no usable date found
    pub no_date: usize,
    pub failed: usize,
}

/// Backfills at most `limit` posts, oldest-first within the pending set.
/// Each post costs one Naver request, so this honours the crawl delay and is
/// meant to be run repeatedly rather than all at once.
pub async fn run(ctx: Arc<AppContext>, limit: i64) -> Result<BackfillReport> {
    let blog_id = ctx.config.naver_blog_id.clone();
    let post_repo = PostRepo::new(ctx.pool.clone());
    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());

    let targets = post_repo.find_needing_date_backfill(&blog_id, limit).await?;
    let mut report = BackfillReport {
        examined: targets.len(),
        ..Default::default()
    };

    if targets.is_empty() {
        info!("backfill_dates: nothing to do");
        return Ok(report);
    }
    info!(count = targets.len(), "backfill_dates: starting");

    for log_no in targets {
        crawler.rate_limit().await;
        match crawler.fetch_post(log_no).await {
            Ok(fetched) => match fetched.published_at {
                Some(published_at) => {
                    post_repo
                        .update_add_date(&blog_id, log_no, published_at)
                        .await?;
                    report.updated += 1;
                    info!(log_no, %published_at, "backfill_dates: updated");
                }
                None => {
                    report.no_date += 1;
                    warn!(log_no, "backfill_dates: no date found on detail page");
                }
            },
            Err(e) => {
                report.failed += 1;
                warn!(log_no, error = %e, "backfill_dates: fetch failed");
            }
        }
    }

    info!(
        updated = report.updated,
        no_date = report.no_date,
        failed = report.failed,
        "backfill_dates: complete"
    );
    Ok(report)
}
