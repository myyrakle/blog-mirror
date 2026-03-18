use std::path::PathBuf;

use config::{Config, Environment, File};
use serde::Deserialize;

use crate::error::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    /// Naver Blog ID (e.g. "sssang97")
    pub naver_blog_id: String,

    /// Local path to the cloned GitHub blog repository
    pub github_repo_path: PathBuf,

    /// Remote URL of the GitHub blog repo (https://github.com/user/repo.git)
    pub github_remote_url: String,

    /// GitHub username for git auth
    pub github_username: String,

    /// GitHub Personal Access Token
    pub github_token: String,

    /// PostgreSQL connection URL
    pub database_url: String,

    /// Delay between Naver requests in milliseconds (default 1000)
    #[serde(default = "default_crawl_delay_ms")]
    pub crawl_delay_ms: u64,
}

fn default_crawl_delay_ms() -> u64 {
    1000
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let cfg = Config::builder()
            // Optional file-based config
            .add_source(File::with_name("blog-mirror").required(false))
            // Environment variables (e.g. NAVER_BLOG_ID, DATABASE_URL)
            .add_source(Environment::default().separator("__").try_parsing(true))
            .build()?;

        Ok(cfg.try_deserialize()?)
    }
}
