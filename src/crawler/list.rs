use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::warn;

use crate::error::{AppError, Result};

use super::NaverCrawler;

/// Raw item returned by Naver's PostTitleListAsync API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostListItem {
    /// Naver returns logNo as a string (e.g. "224219853968")
    #[serde(deserialize_with = "deserialize_string_or_i64")]
    pub log_no: i64,
    pub title: String,
    #[serde(deserialize_with = "deserialize_optional_string_or_i32", default)]
    pub category_no: Option<i32>,
    pub add_date: Option<String>,
}

fn deserialize_string_or_i64<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct StringOrI64;
    impl<'de> Visitor<'de> for StringOrI64 {
        type Value = i64;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an integer or string containing an integer")
        }
        fn visit_i64<E: Error>(self, v: i64) -> std::result::Result<i64, E> { Ok(v) }
        fn visit_u64<E: Error>(self, v: u64) -> std::result::Result<i64, E> { Ok(v as i64) }
        fn visit_str<E: Error>(self, v: &str) -> std::result::Result<i64, E> {
            v.parse().map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
        }
    }
    deserializer.deserialize_any(StringOrI64)
}

fn deserialize_optional_string_or_i32<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Unexpected, Visitor};
    use std::fmt;

    struct OptStringOrI32;
    impl<'de> Visitor<'de> for OptStringOrI32 {
        type Value = Option<i32>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an integer, string containing an integer, or null")
        }
        fn visit_none<E: Error>(self) -> std::result::Result<Option<i32>, E> { Ok(None) }
        fn visit_unit<E: Error>(self) -> std::result::Result<Option<i32>, E> { Ok(None) }
        fn visit_i64<E: Error>(self, v: i64) -> std::result::Result<Option<i32>, E> { Ok(Some(v as i32)) }
        fn visit_u64<E: Error>(self, v: u64) -> std::result::Result<Option<i32>, E> { Ok(Some(v as i32)) }
        fn visit_str<E: Error>(self, v: &str) -> std::result::Result<Option<i32>, E> {
            if v.is_empty() {
                return Ok(None);
            }
            v.parse::<i32>()
                .map(Some)
                .map_err(|_| E::invalid_value(Unexpected::Str(v), &self))
        }
        fn visit_some<D2: serde::Deserializer<'de>>(
            self,
            d: D2,
        ) -> std::result::Result<Option<i32>, D2::Error> {
            deserialize_optional_string_or_i32(d)
        }
    }
    deserializer.deserialize_any(OptStringOrI32)
}

impl PostListItem {
    /// Parse the add_date string into DateTime<Utc>.
    /// Handles both absolute formats ("2024. 01. 15. 10:30") and
    /// relative formats ("방금", "N분 전", "N시간 전", "어제", "N일 전").
    pub fn parsed_add_date(&self) -> Option<DateTime<Utc>> {
        let s = self.add_date.as_deref()?.trim();
        let now = chrono::Utc::now();

        // --- Relative formats ---
        if s == "방금" {
            return Some(now);
        }
        if s == "어제" {
            return Some(now - chrono::Duration::days(1));
        }
        if let Some(n) = parse_relative(s, "분 전") {
            return Some(now - chrono::Duration::minutes(n));
        }
        if let Some(n) = parse_relative(s, "시간 전") {
            return Some(now - chrono::Duration::hours(n));
        }
        if let Some(n) = parse_relative(s, "일 전") {
            return Some(now - chrono::Duration::days(n));
        }

        // --- Absolute format: "2024. 01. 15. 10:30" ---
        // Normalize by replacing ". " with "-" and removing remaining "."
        let normalized = s.replace(". ", "-").replace('.', "").trim().to_string();
        for fmt in &["%Y-%m-%d-%H:%M", "%Y- %m- %d- %H:%M"] {
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&normalized, fmt) {
                return Some(naive.and_utc());
            }
        }

        // --- Absolute date only: "2024. 01. 15." ---
        if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y. %m. %d.") {
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
        }

        warn!(raw = s, "Could not parse Naver add_date");
        None
    }
}

/// Parses "N<suffix>" (e.g. "16시간 전") and returns N as i64.
fn parse_relative(s: &str, suffix: &str) -> Option<i64> {
    s.strip_suffix(suffix)?.trim().parse::<i64>().ok()
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
        let text = sanitize_json(&text);

        let parsed: PostListResponse =
            serde_json::from_str(&text).map_err(|e| AppError::Parse(e.to_string()))?;

        Ok(parsed.post_list)
    }


}

/// Naver's API occasionally returns invalid JSON escape sequences (e.g. `\s`, `\p`).
/// This function replaces any `\x` where x is not a valid JSON escape character
/// with a space so that serde_json can parse the response.
fn sanitize_json(s: &str) -> String {
    let valid_escapes = ['"', '\\', '/', 'b', 'f', 'n', 'r', 't', 'u'];
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some(&next) if valid_escapes.contains(&next) => {
                    out.push(ch);
                }
                Some(_) => {
                    // Invalid escape: drop the backslash
                    out.push(' ');
                    chars.next(); // consume the invalid char too
                    continue;
                }
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    out
}
