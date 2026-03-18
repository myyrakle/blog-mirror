use tracing::info;

use crate::error::Result;

use super::NaverCrawler;

impl NaverCrawler {
    /// Fetches the full HTML of a Naver blog post.
    pub async fn fetch_post_html(&self, log_no: i64) -> Result<String> {
        let url = format!(
            "https://blog.naver.com/PostView.naver?blogId={}&logNo={}&redirect=Dlog&widgetTypeCall=true&noTrackingCode=true&directAccess=false",
            self.config.naver_blog_id, log_no
        );
        info!(log_no, "Fetching Naver post HTML");
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;
        Ok(html)
    }
}
