use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use scraper::{Html, Selector};
use tracing::info;

use crate::error::Result;

use super::NaverCrawler;

/// Naver publishes every timestamp in Korea Standard Time, with no offset
/// attached to the markup.
const KST_OFFSET_SECONDS: i32 = 9 * 3600;

/// A fetched post: the content HTML plus the exact publish time, which only
/// the detail page carries (the list API gives a date with no time).
pub struct FetchedPost {
    pub html: String,
    pub published_at: Option<DateTime<Utc>>,
}

impl NaverCrawler {
    /// Fetches a Naver blog post and returns only the content container's
    /// inner HTML. Uses the mobile URL which embeds the content directly.
    pub async fn fetch_post_html(&self, log_no: i64) -> Result<String> {
        Ok(self.fetch_post(log_no).await?.html)
    }

    /// Fetches a post, returning both its content and its publish time.
    pub async fn fetch_post(&self, log_no: i64) -> Result<FetchedPost> {
        let url = format!(
            "https://m.blog.naver.com/{}/{}",
            self.config.naver_blog_id, log_no
        );
        info!(log_no, "Fetching Naver post HTML (mobile)");
        let resp = self.client.get(&url).send().await?;
        let html = resp.text().await?;

        Ok(FetchedPost {
            published_at: extract_publish_date(&html),
            html: extract_main_content(&html),
        })
    }
}

/// Reads the publish timestamp out of the detail page.
///
/// ```html
/// <p class="blog_date"><span class="txt">2017. 12. 2. 15:54</span></p>
/// ```
///
/// This is the only place the time of day is available — the post list API
/// returns `"2017. 12. 2."` with no time, which leaves every post that day
/// sorting at midnight in an arbitrary order.
fn extract_publish_date(html: &str) -> Option<DateTime<Utc>> {
    let document = Html::parse_document(html);
    let sel = Selector::parse("p.blog_date, .blog_date .txt, .se_publishDate").ok()?;
    for el in document.select(&sel) {
        let text = el.text().collect::<String>();
        if let Some(dt) = parse_kst_datetime(text.trim()) {
            return Some(dt);
        }
    }
    None
}

/// Parses Naver's `"2017. 12. 2. 15:54"` (and the date-only variant) as KST.
pub fn parse_kst_datetime(s: &str) -> Option<DateTime<Utc>> {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let kst = FixedOffset::east_opt(KST_OFFSET_SECONDS)?;

    for fmt in ["%Y. %m. %d. %H:%M:%S", "%Y. %m. %d. %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&s, fmt) {
            return kst.from_local_datetime(&naive).single().map(|d| d.to_utc());
        }
    }

    // Date only — midnight KST. Better than nothing, but carries no ordering
    // information within the day.
    if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y. %m. %d.") {
        let naive = date.and_hms_opt(0, 0, 0)?;
        return kst.from_local_datetime(&naive).single().map(|d| d.to_utc());
    }

    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kst_datetime_as_utc() {
        // 2017-12-02 15:54 KST == 06:54 UTC. Reading it as UTC (the old
        // behaviour) put every post 9 hours early.
        let dt = parse_kst_datetime("2017. 12. 2. 15:54").unwrap();
        assert_eq!(dt.to_rfc3339(), "2017-12-02T06:54:00+00:00");
    }

    #[test]
    fn parses_date_only_as_kst_midnight() {
        let dt = parse_kst_datetime("2026. 2. 22.").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-02-21T15:00:00+00:00");
    }

    #[test]
    fn rejects_unparseable_date() {
        assert!(parse_kst_datetime("방금").is_none());
        assert!(parse_kst_datetime("").is_none());
    }

    #[test]
    fn extracts_publish_date_from_detail_page() {
        let html = r#"<html><body>
            <div class="blog_header"><p class="blog_date"><span class="txt">2017. 12. 2. 15:54</span></p></div>
            <div class="se-main-container"><p>본문</p></div>
        </body></html>"#;
        let dt = extract_publish_date(html).expect("date should be found");
        assert_eq!(dt.to_rfc3339(), "2017-12-02T06:54:00+00:00");
    }

    #[test]
    fn missing_date_is_none_not_a_guess() {
        let html = r#"<html><body><div class="se-main-container"><p>본문</p></div></body></html>"#;
        assert!(extract_publish_date(html).is_none());
    }
}
