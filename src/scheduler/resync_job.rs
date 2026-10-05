//! Manual re-synchronisation of individual posts.
//!
//! Used when a post was edited on Naver after it had already been mirrored:
//! optionally re-download the body from Naver, clear the replication marker,
//! then run the normal replicate job so the `.md` file is rewritten and pushed.

use std::{collections::HashMap, sync::Arc};

use tracing::{info, warn};

use crate::{
    context::AppContext,
    crawler::NaverCrawler,
    db::{category_repo::CategoryRepo, post_repo::PostRepo},
    error::Result,
    scheduler::replicate_job,
};

#[derive(Debug)]
pub struct ResyncRequest {
    /// Posts to re-synchronise
    pub log_nos: Vec<i64>,
    /// Re-download the post body from Naver before republishing
    pub refetch: bool,
}

#[derive(Debug, Default)]
pub struct ResyncReport {
    pub requested: usize,
    /// Posts whose body was re-downloaded from Naver
    pub refetched: usize,
    /// Posts whose body could not be re-downloaded
    pub refetch_failed: usize,
    /// Posts queued for republishing (replication marker cleared)
    pub queued: usize,
    /// Requested posts sitting in a category with should_mirror = false
    pub skipped_not_mirrored: Vec<i64>,
    pub replicate: replicate_job::ReplicateReport,
}

pub async fn run(ctx: Arc<AppContext>, request: ResyncRequest) -> Result<ResyncReport> {
    let blog_id = ctx.config.naver_blog_id.clone();
    let post_repo = PostRepo::new(ctx.pool.clone());
    let category_repo = CategoryRepo::new(ctx.pool.clone());
    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());

    let mut report = ResyncReport {
        requested: request.log_nos.len(),
        ..Default::default()
    };

    let posts = post_repo.find_by_log_nos(&blog_id, &request.log_nos).await?;
    if posts.is_empty() {
        warn!("resync_job: none of the requested posts exist in the DB");
        return Ok(report);
    }
    info!(count = posts.len(), refetch = request.refetch, "resync_job: starting");

    // Warn up front about posts that the replicate job will not pick up.
    let mirror_flags: HashMap<i32, bool> = category_repo
        .find_with_stats(&blog_id)
        .await?
        .into_iter()
        .map(|c| (c.category_no, c.should_mirror))
        .collect();

    for post in &posts {
        let mirrored = post
            .category_no
            .and_then(|n| mirror_flags.get(&n).copied())
            .unwrap_or(false);
        if !mirrored {
            report.skipped_not_mirrored.push(post.log_no);
            warn!(
                log_no = post.log_no,
                category_no = ?post.category_no,
                "resync_job: category is not marked should_mirror, post will not be published"
            );
        }
    }

    if request.refetch {
        for post in &posts {
            crawler.rate_limit().await;
            match crawler.fetch_post_html(post.log_no).await {
                Ok(html) => {
                    post_repo.save_body(&blog_id, post.log_no, &html).await?;
                    report.refetched += 1;
                    info!(log_no = post.log_no, "resync_job: body refetched");
                }
                Err(e) => {
                    report.refetch_failed += 1;
                    warn!(log_no = post.log_no, error = %e, "resync_job: refetch failed");
                }
            }
        }
    }

    let log_nos: Vec<i64> = posts.iter().map(|p| p.log_no).collect();
    report.queued = post_repo.reset_replication(&blog_id, &log_nos).await? as usize;
    info!(queued = report.queued, "resync_job: replication markers cleared");

    // Scoped to the requested posts: a re-sync shouldn't also publish every
    // other post that happens to be sitting unreplicated.
    report.replicate = replicate_job::run_for(ctx, &log_nos).await?;

    info!(
        refetched = report.refetched,
        replicated = report.replicate.replicated,
        "resync_job: complete"
    );
    Ok(report)
}
