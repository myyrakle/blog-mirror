use std::path::Path;

use chrono::{DateTime, Utc};

use crate::error::Result;

/// Represents a post ready to be written as a Zola markdown file.
#[derive(Debug)]
pub struct MirroredPost {
    pub log_no: i64,
    pub title: String,
    pub category_name: Option<String>,
    pub markdown_body: String,
    pub add_date: Option<DateTime<Utc>>,
    pub category_no: Option<i32>,
}

/// Writes a Zola-formatted markdown file for the given post.
/// File is placed at `{repo_path}/content/blog/{log_no}.md`.
pub fn write_post(repo_path: &Path, post: &MirroredPost) -> Result<()> {
    let blog_dir = repo_path.join("content").join("blog");
    std::fs::create_dir_all(&blog_dir)?;

    let file_path = blog_dir.join(format!("{}.md", post.log_no));
    let content = render_post(post);
    std::fs::write(&file_path, content)?;
    Ok(())
}

fn render_post(post: &MirroredPost) -> String {
    let date = post
        .add_date
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let category_line = post
        .category_name
        .as_deref()
        .map(|c| format!("categories = [\"{}\"]\n", escape_toml_string(c)))
        .unwrap_or_else(|| "categories = []\n".to_string());

    let title_escaped = escape_toml_string(&post.title);

    format!(
        r#"+++
title = "{title}"
date = {date}
[taxonomies]
{category_line}tags = []
[extra]
naver_log_no = {log_no}
{category_no_line}+++

{body}
"#,
        title = title_escaped,
        date = date,
        category_line = category_line,
        log_no = post.log_no,
        category_no_line = post
            .category_no
            .map(|n| format!("naver_category_no = {}\n", n))
            .unwrap_or_default(),
        body = post.markdown_body.trim(),
    )
}

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_post() {
        let post = MirroredPost {
            log_no: 12345,
            title: "Test \"Post\"".to_string(),
            category_name: Some("Programming".to_string()),
            markdown_body: "Hello World".to_string(),
            add_date: None,
            category_no: Some(42),
        };
        let rendered = render_post(&post);
        assert!(rendered.contains("title = \"Test \\\"Post\\\"\""));
        assert!(rendered.contains("naver_log_no = 12345"));
        assert!(rendered.contains("categories = [\"Programming\"]"));
        assert!(rendered.contains("Hello World"));
    }
}
