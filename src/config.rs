use std::path::PathBuf;

use config::{Config, Environment, File};
use serde::Deserialize;

use crate::error::{AppError, Result};

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

    /// Port the admin dashboard listens on (default 8080)
    #[serde(default = "default_web_port")]
    pub web_port: u16,

    /// HTTP Basic auth username for the dashboard.
    /// Auth is disabled unless both username and password are set.
    #[serde(default)]
    pub web_username: Option<String>,

    /// HTTP Basic auth password for the dashboard
    #[serde(default)]
    pub web_password: Option<String>,
}

fn default_crawl_delay_ms() -> u64 {
    1000
}

fn default_web_port() -> u16 {
    8080
}

impl AppConfig {
    /// Dashboard credentials, or `None` when auth is deliberately disabled.
    ///
    /// Setting only one of the two is rejected rather than silently falling
    /// back to no auth — otherwise adding just `WEB_USERNAME` to an existing
    /// deployment would leave the admin API open.
    pub fn web_credentials(&self) -> Result<Option<(String, String)>> {
        let user = self.web_username.as_deref().unwrap_or("").trim();
        let pass = self.web_password.as_deref().unwrap_or("").trim();

        match (user.is_empty(), pass.is_empty()) {
            (true, true) => Ok(None),
            (false, false) => Ok(Some((user.to_string(), pass.to_string()))),
            (true, false) => Err(AppError::Parse(
                "WEB_PASSWORD만 설정되어 있습니다. WEB_USERNAME도 함께 설정하거나 둘 다 지우세요.".into(),
            )),
            (false, true) => Err(AppError::Parse(
                "WEB_USERNAME만 설정되어 있습니다. WEB_PASSWORD도 함께 설정하거나 둘 다 지우세요.".into(),
            )),
        }
    }

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
