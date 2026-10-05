use std::sync::Arc;

use crate::{context::AppContext, web::jobs::JobManager};

pub struct WebState {
    pub ctx: Arc<AppContext>,
    pub jobs: Arc<JobManager>,
    /// Optional HTTP Basic credentials; when `None` the dashboard is open.
    pub auth: Option<(String, String)>,
}
