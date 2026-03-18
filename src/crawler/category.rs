use scraper::{Html, Selector};
use tracing::info;

use crate::error::{AppError, Result};

use super::NaverCrawler;

#[derive(Debug)]
pub struct CategoryItem {
    pub category_no: i32,
    pub parent_no: Option<i32>,
    pub name: String,
    pub post_count: i32,
}

impl NaverCrawler {
    /// Fetches the category list from Naver's WidgetListAsync API.
    /// Returns all categories with their actual names.
    pub async fn fetch_categories(&self) -> Result<Vec<CategoryItem>> {
        let url = format!(
            "https://blog.naver.com/mylog/WidgetListAsync.naver?blogId={}&isCategoryOpen=true&enableWidgetKeys=category&listNumVisitor=0&isVisitorOpen=false&isBuddyOpen=false&skinId=0&skinType=C&isEnglish=true&listNumComment=0&writingMaterialListType=1",
            self.config.naver_blog_id
        );

        info!("Fetching category list");
        let resp = self
            .client
            .get(&url)
            .header(
                "Referer",
                format!("https://blog.naver.com/{}", self.config.naver_blog_id),
            )
            .send()
            .await?;
        let body = resp.text().await?;

        parse_categories(&body).map_err(AppError::Parse)
    }
}

/// Parses category items from the WidgetListAsync response.
/// The response is a JS-like object; we extract the HTML content of the `category` key
/// and parse `<li>` items inside it.
fn parse_categories(body: &str) -> std::result::Result<Vec<CategoryItem>, String> {
    // Extract the HTML string value after `category : { content : '`
    let start_marker = "category : { content : '";
    let start = body
        .find(start_marker)
        .ok_or("category content not found")?
        + start_marker.len();

    // Find the closing `' }` — the content ends at the next unescaped single-quote
    let content_raw = &body[start..];
    let end = find_closing_quote(content_raw).ok_or("category content end not found")?;
    let html_escaped = &content_raw[..end];

    // Unescape JS single-quote escapes and HTML entities
    let html = html_escaped.replace("\\'", "'");

    let document = Html::parse_fragment(&html);

    // Skip "전체보기" (categoryNo=0), parse real category <li> items
    let li_sel = Selector::parse("li").map_err(|e| e.to_string())?;
    let a_sel = Selector::parse("a[href*='categoryNo']").map_err(|e| e.to_string())?;
    let num_sel = Selector::parse("span.num").map_err(|e| e.to_string())?;

    let mut categories = Vec::new();

    for li in document.select(&li_sel) {
        // Get the anchor with categoryNo
        let Some(a) = li.select(&a_sel).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or("");

        let category_no = extract_query_param(href, "categoryNo")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);

        if category_no == 0 {
            continue; // skip "전체보기"
        }

        let parent_no = extract_query_param(href, "parentCategoryNo")
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|&n| n != category_no); // parentNo == categoryNo means it's a root category

        // Category name: text of the anchor, excluding the post count span text
        let count_text = li
            .select(&num_sel)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let post_count = count_text
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .parse::<i32>()
            .unwrap_or(0);

        // Name = anchor text minus the count suffix
        let raw_name: String = a.text().collect::<String>();
        let name = raw_name.trim().to_string();

        if name.is_empty() {
            continue;
        }

        categories.push(CategoryItem {
            category_no,
            parent_no,
            name,
            post_count,
        });
    }

    info!(count = categories.len(), "Parsed categories");
    Ok(categories)
}

/// Finds the index of the first unescaped single-quote in the string.
fn find_closing_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped char
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn extract_query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let key_eq = format!("{}=", key);
    let start = url.find(key_eq.as_str())? + key_eq.len();
    let rest = &url[start..];
    let end = rest.find(['&', ' ', '"', '\'']).unwrap_or(rest.len());
    Some(&rest[..end])
}
