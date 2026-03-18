use sqlx::PgPool;

use crate::error::Result;

pub struct CursorRepo {
    pool: PgPool,
}

impl CursorRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the last processed log_no cursor. Returns 0 if no cursor exists.
    pub async fn get_cursor(&self, blog_id: &str) -> Result<i64> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT last_log_no FROM sync_cursor WHERE blog_id = $1",
        )
        .bind(blog_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(v,)| v).unwrap_or(0))
    }

    /// Upserts the cursor for the given blog_id.
    pub async fn update_cursor(&self, blog_id: &str, log_no: i64) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sync_cursor (blog_id, last_log_no, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (blog_id)
            DO UPDATE SET last_log_no = EXCLUDED.last_log_no, updated_at = NOW()
            "#,
        )
        .bind(blog_id)
        .bind(log_no)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
