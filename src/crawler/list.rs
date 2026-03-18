use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{info, warn};

use crate::error::{AppError, Result};

use super::NaverCrawler;

/// Raw item returned by Naver's PostTitleListAsync API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostListItem {
    pub log_no: i64,
    pub title: String,
    pub category_no: Option<i32>,
    pub add_date: Option<String>,
}

impl PostListItem {
    /// Parse the add_date string (e.g. "2024. 01. 15. 10:30") into DateTime<Utc>.
    pub fn parsed_add_date(&self) -> Option<DateTime<Utc>> {
        let s = self.add_date.as_deref()?;
        // Naver format: "2024. 01. 15. 10:30" or "2024.01.15. 10:30"
        // Normalize: remove spaces around periods, strip trailing period
        let normalized = s
            .replace(". ", "-")
            .replace(".", "")
            .trim()
            .to_string();
        // Attempt parsing as "YYYY-MM-DD-HH:MM"
        let normalized = normalized.replacen('-', "", 0); // keep dashes

        // Try common Naver date formats
        let formats = [
            "%Y- %m- %d- %H:%M",
            "%Y-%m-%d-%H:%M",
            "%Y. %m. %d. %H:%M",
        ];
        for fmt in &formats {
            if let Ok(naive) =
                chrono::NaiveDateTime::parse_from_str(&normalized, fmt)
            {
                return Some(naive.and_utc());
            }
        }
        // fallback: try parsing just the date portion
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s.trim(), "%Y. %m. %d.") {
            return Some(
                date.and_hms_opt(0, 0, 0)
                    .map(|dt| dt.and_utc())
                    .unwrap_or_default(),
            );
        }
        warn!(raw = s, "Could not parse Naver add_date");
        None
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostListResponse {
    post_list: Vec<PostListItem>,
}

impl NaverCrawler {
    /// Fetches a single page from Naver's list API.
    pub async fn fetch_post_list_page(
        &self,
        page: u32,
        count_per_page: u32,
    ) -> Result<Vec<PostListItem>> {
        let url = format!(
            "https://blog.naver.com/PostTitleListAsync.naver?blogId={}&viewdate=&currentPage={}&categoryNo=0&parentCategoryNo=0&countPerPage={}",
            self.config.naver_blog_id, page, count_per_page
        );
        let resp = self.client.get(&url).send().await?;
        let text = resp.text().await?;

        let parsed: PostListResponse =
            serde_json::from_str(&text).map_err(|e| AppError::Parse(e.to_string()))?;

        Ok(parsed.post_list)
    }

    /// Paginates from page 1, stopping when a post with log_no <= cursor is found
    /// or when a page returns fewer items than count_per_page.
    /// Pass cursor = 0 to fetch everything (initial sync).
    pub async fn fetch_all_posts_until(&self, cursor: i64) -> Result<Vec<PostListItem>> {
        let count_per_page = 30u32;
        let mut all = Vec::new();
        let mut page = 1u32;

        loop {
            info!(page, cursor, "Fetching Naver post list page");
            let items = self.fetch_post_list_page(page, count_per_page).await?;
            let fetched = items.len();

            let mut done = false;
            for item in items {
                if item.log_no <= cursor {
                    done = true;
                    break;
                }
                all.push(item);
            }

            if done || fetched < count_per_page as usize {
                break;
            }

            page += 1;
            self.rate_limit().await;
        }

        info!(total = all.len(), "Finished fetching post list");
        Ok(all)
    }
}
