use scraper::{Html, Selector};
use tracing::info;

use crate::error::Result;

use super::NaverCrawler;

impl NaverCrawler {
    /// Fetches a Naver blog post and returns only the `se-main-container` inner HTML.
    /// Uses the mobile URL which embeds the content directly in the HTML.
    pub async fn fetch_post_html(&self, log_no: i64) -> Result<String> {
        let url = format!(
            "https://m.blog.naver.com/{}/{}",
            self.config.naver_blog_id, log_no
        );
        info!(log_no, "Fetching Naver post HTML (mobile)");
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;

        Ok(extract_main_content(&html))
    }
}

/// Extracts the inner HTML of the post content container.
/// Tries SE3 and various legacy selectors in order.
/// Falls back to the full HTML if none is found.
fn extract_main_content(html: &str) -> String {
    let document = Html::parse_document(html);

    // SE3, legacy postViewArea, mobile SE2 content areas
    // NOTE: .post_ct must come before ._postView because ._postView is the outer
    // wrapper containing navigation, while .post_ct is the actual article content.
    for selector_str in &[
        ".se-main-container",
        "#postViewArea",
        ".post-view",
        ".post_ct",
        "._postView",
    ] {
        if let Ok(sel) = Selector::parse(selector_str)
            && let Some(el) = document.select(&sel).next()
        {
            return el.inner_html();
        }
    }

    // Fallback: return full HTML if no known container found
    html.to_string()
}
