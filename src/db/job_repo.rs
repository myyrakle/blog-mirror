use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct JobRunRecord {
    pub id: i32,
    pub kind: String,
    pub status: String,
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

pub struct JobRepo {
    pool: PgPool,
}

impl JobRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Records the start of a job run and returns its id.
    pub async fn start(&self, blog_id: &str, kind: &str) -> Result<i32> {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO job_runs (blog_id, kind, status) VALUES ($1, $2, 'running') RETURNING id",
        )
        .bind(blog_id)
        .bind(kind)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Marks a job run as finished with the given status, message and captured log.
    pub async fn finish(&self, id: i32, status: &str, message: &str, log: &str) -> Result<()> {
        sqlx::query(
            "UPDATE job_runs SET status = $2, message = $3, log = $4, finished_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(message)
        .bind(log)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Any run still marked `running` at startup was killed mid-flight by a restart.
    pub async fn mark_orphans_interrupted(&self, blog_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE job_runs SET status = 'interrupted', finished_at = NOW()
             WHERE blog_id = $1 AND status = 'running'",
        )
        .bind(blog_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn recent(&self, blog_id: &str, limit: i64) -> Result<Vec<JobRunRecord>> {
        let rows = sqlx::query_as::<_, JobRunRecord>(
            "SELECT id, kind, status, message, started_at, finished_at
             FROM job_runs WHERE blog_id = $1 ORDER BY started_at DESC LIMIT $2",
        )
        .bind(blog_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn log(&self, blog_id: &str, id: i32) -> Result<Option<String>> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT log FROM job_runs WHERE blog_id = $1 AND id = $2")
                .bind(blog_id)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.and_then(|(v,)| v))
    }
}
