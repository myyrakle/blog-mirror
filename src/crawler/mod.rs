pub mod category;
pub mod detail;
pub mod list;

use std::sync::Arc;

use reqwest::Client;
use tokio::time::{Duration, sleep};

use crate::config::AppConfig;

pub struct NaverCrawler {
    pub client: Client,
    pub config: Arc<AppConfig>,
}

impl NaverCrawler {
    pub fn new(config: Arc<AppConfig>, client: Client) -> Self {
        Self { client, config }
    }

    /// Waits for the configured crawl delay to avoid rate-limiting.
    pub async fn rate_limit(&self) {
        sleep(Duration::from_millis(self.config.crawl_delay_ms)).await;
    }
}
