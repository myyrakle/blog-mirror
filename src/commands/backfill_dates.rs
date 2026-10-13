use std::sync::Arc;

use crate::{
    context::AppContext, error::Result, scheduler::backfill_dates_job,
    scheduler::backfill_dates_job::BackfillReport,
};

/// One-shot: fill in publish times missing from the post list API.
pub async fn run(ctx: Arc<AppContext>, limit: i64) -> Result<BackfillReport> {
    backfill_dates_job::run(ctx, limit).await
}
