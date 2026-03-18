use std::sync::Arc;

use tracing::{info, warn};

use crate::{
    context::AppContext,
    converter::{ImageHandler, convert_html_to_markdown},
    crawler::NaverCrawler,
    db::{category_repo::CategoryRepo, post_repo::PostRepo},
    error::Result,
    github::{GitRepo, MirroredPost, write_post},
};

/// Replication job:
/// 1. Pull latest changes
/// 2. Read mirror categories from DB (should_mirror = TRUE)
/// 3. Find unreplicated posts in those categories
/// 4. For each post: fetch HTML → convert → write .md → download images
/// 5. Commit and push all at once
pub async fn run(ctx: Arc<AppContext>) -> Result<()> {
    info!("replicate_job: starting");

    let git_repo = GitRepo::open_or_clone(ctx.config.clone())?;
    if let Err(e) = git_repo.pull() {
        warn!(error = %e, "replicate_job: git pull failed (continuing)");
    }

    let post_repo = PostRepo::new(ctx.pool.clone());
    let category_repo = CategoryRepo::new(ctx.pool.clone());
    let crawler = NaverCrawler::new(ctx.config.clone(), ctx.http.clone());
    let image_handler = ImageHandler::new(ctx.http.clone(), &git_repo.repo_path());

    // Read mirror targets from DB (manually toggled via should_mirror flag)
    let mirror_cats = category_repo
        .find_mirror_categories(&ctx.config.naver_blog_id)
        .await?;
    if mirror_cats.is_empty() {
        info!("replicate_job: no categories with should_mirror=true in DB, skipping");
        return Ok(());
    }
    let mirror_category_nos: Vec<i32> = mirror_cats.iter().map(|c| c.category_no).collect();
    info!(categories = ?mirror_category_nos, "replicate_job: mirror categories");

    let posts = post_repo
        .find_unreplicated_in_categories(&ctx.config.naver_blog_id, &mirror_category_nos)
        .await?;

    if posts.is_empty() {
        info!("replicate_job: no posts to replicate");
        return Ok(());
    }
    info!(count = posts.len(), "replicate_job: posts to replicate");

    // Build category name lookup from already-fetched mirror_cats
    let cat_name_map: std::collections::HashMap<i32, String> = mirror_cats
        .into_iter()
        .map(|c| (c.category_no, c.name))
        .collect();

    let mut replicated_count = 0usize;

    for post in &posts {
        info!(log_no = post.log_no, title = %post.title, "Replicating post");

        // Use stored body if available, otherwise fetch from Naver
        let html = if let Some(body) = post.body.clone() {
            body
        } else {
            match crawler.fetch_post_html(post.log_no).await {
                Ok(h) => h,
                Err(e) => {
                    warn!(log_no = post.log_no, error = %e, "Failed to fetch post HTML");
                    post_repo
                        .mark_replication_error(
                            &ctx.config.naver_blog_id,
                            post.log_no,
                            &e.to_string(),
                        )
                        .await?;
                    continue;
                }
            }
        };

        // Convert HTML to Markdown
        let raw_markdown = convert_html_to_markdown(&html);

        // Download and rewrite image URLs
        let markdown = match image_handler.rewrite_markdown_images(&raw_markdown).await {
            Ok(md) => md,
            Err(e) => {
                warn!(log_no = post.log_no, error = %e, "Image rewrite failed, using raw markdown");
                raw_markdown
            }
        };

        let category_name = post.category_no.and_then(|n| cat_name_map.get(&n).cloned());

        let mirrored = MirroredPost {
            log_no: post.log_no,
            title: post.title.clone(),
            category_name,
            markdown_body: markdown,
            add_date: post.add_date,
            category_no: post.category_no,
        };

        if let Err(e) = write_post(&git_repo.repo_path(), &mirrored) {
            warn!(log_no = post.log_no, error = %e, "Failed to write post file");
            post_repo
                .mark_replication_error(&ctx.config.naver_blog_id, post.log_no, &e.to_string())
                .await?;
            continue;
        }

        post_repo
            .mark_replicated(&ctx.config.naver_blog_id, post.log_no)
            .await?;
        replicated_count += 1;

        crawler.rate_limit().await;
    }

    if replicated_count > 0 {
        info!(replicated_count, "Committing and pushing to GitHub");
        git_repo.add_all()?;

        if git_repo.has_staged_changes()? {
            let msg = format!("mirror: add {} post(s) from Naver blog", replicated_count);
            git_repo.commit(&msg)?;
            git_repo.push()?;
            info!("replicate_job: push complete");
        } else {
            info!("replicate_job: no changes to push");
        }
    }

    info!(replicated_count, "replicate_job: complete");
    Ok(())
}
