use std::sync::Arc;

use reqwest::{Client, ClientBuilder, header};
use sqlx::PgPool;

use crate::config::AppConfig;
use crate::error::Result;

/// Shared application context passed to all jobs and commands.
#[derive(Clone)]
pub struct AppContext {
    pub config: Arc<AppConfig>,
    pub pool: PgPool,
    pub http: Client,
}

impl AppContext {
    pub fn new(config: Arc<AppConfig>, pool: PgPool) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            ),
        );

        let http = ClientBuilder::new()
            .cookie_store(true)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(crate::error::AppError::Http)?;

        Ok(Self { config, pool, http })
    }
}
