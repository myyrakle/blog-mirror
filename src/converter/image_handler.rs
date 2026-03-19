use std::path::{Path, PathBuf};

use reqwest::Client;
use tracing::{info, warn};
use crate::error::Result;

pub struct ImageHandler {
    client: Client,
    images_dir: PathBuf,
}

impl ImageHandler {
    pub fn new(client: Client, repo_path: &Path) -> Self {
        let images_dir = repo_path.join("static").join("images");
        Self { client, images_dir }
    }

    /// Downloads an image from `src_url`, saves it to `static/images/`,
    /// and returns the markdown-usable path `/images/{filename}`.
    pub async fn download_and_save(&self, src_url: &str) -> Result<String> {
        let filename = derive_filename(src_url);
        let dest_path = self.images_dir.join(&filename);

        // Skip if already downloaded
        if dest_path.exists() {
            return Ok(format!("/images/{}", filename));
        }

        std::fs::create_dir_all(&self.images_dir)?;

        info!(src_url, dest = ?dest_path, "Downloading image");
        let resp = self
            .client
            .get(src_url)
            .header("Referer", "https://blog.naver.com/")
            .send()
            .await?;
        let bytes = resp.bytes().await?;
        std::fs::write(&dest_path, &bytes)?;

        Ok(format!("/images/{}", filename))
    }

    /// Finds all `![...](url)` patterns in markdown and rewrites external image URLs
    /// to local paths, downloading images in the process.
    pub async fn rewrite_markdown_images(&self, markdown: &str) -> Result<String> {
        let mut result = markdown.to_string();
        let mut offset = 0usize;

        // Find all markdown image references: ![alt](url)
        while let Some(start) = result[offset..].find("![") {
            let abs_start = offset + start;
            let after_bang = abs_start + 2;

            // Find closing ]( and )
            let Some(bracket_close) = result[after_bang..].find("](") else {
                offset = abs_start + 1;
                continue;
            };
            let url_start = after_bang + bracket_close + 2;

            let Some(paren_close) = result[url_start..].find(')') else {
                offset = abs_start + 1;
                continue;
            };
            let url_end = url_start + paren_close;
            let url = result[url_start..url_end].to_string();

            // Only rewrite external (http/https) URLs
            if url.starts_with("http://") || url.starts_with("https://") {
                match self.download_and_save(&url).await {
                    Ok(local_path) => {
                        result.replace_range(url_start..url_end, &local_path);
                        offset = url_start + local_path.len() + 1;
                    }
                    Err(e) => {
                        warn!(url, error = %e, "Failed to download image, keeping original URL");
                        offset = url_end + 1;
                    }
                }
            } else {
                offset = url_end + 1;
            }
        }

        Ok(result)
    }
}

/// Derives a unique local filename from a URL.
/// Always uses a hash of the full URL to avoid collisions (Naver images all end in `image.png`).
/// Preserves the original file extension.
fn derive_filename(url: &str) -> String {
    let hash = simple_hash(url);
    let ext = extract_extension(url).unwrap_or("jpg");
    format!("img_{:016x}.{}", hash, ext)
}

/// Extracts the file extension from the URL path, ignoring query params.
fn extract_extension(url: &str) -> Option<&str> {
    let path = if let Some(q) = url.find('?') {
        &url[..q]
    } else {
        url
    };
    let last_segment = path.rsplit('/').next()?;
    let ext = last_segment.rsplit('.').next()?;
    // Only accept short, sane extensions
    if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(ext)
    } else {
        None
    }
}

fn simple_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
