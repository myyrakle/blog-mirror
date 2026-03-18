use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct PostRecord {
    pub id: i32,
    pub blog_id: String,
    pub log_no: i64,
    pub title: String,
    pub category_no: Option<i32>,
    pub add_date: Option<DateTime<Utc>>,
    pub body: Option<String>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub replicated_at: Option<DateTime<Utc>>,
    pub replication_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct UpsertPost {
    pub blog_id: String,
    pub log_no: i64,
    pub title: String,
    pub category_no: Option<i32>,
    pub add_date: Option<DateTime<Utc>>,
}

pub struct PostRepo {
    pool: PgPool,
}

impl PostRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, record: &UpsertPost) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO posts (blog_id, log_no, title, category_no, add_date, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (blog_id, log_no)
            DO UPDATE SET
                title       = EXCLUDED.title,
                category_no = EXCLUDED.category_no,
                add_date    = EXCLUDED.add_date,
                updated_at  = NOW()
            "#,
        )
        .bind(&record.blog_id)
        .bind(record.log_no)
        .bind(&record.title)
        .bind(record.category_no)
        .bind(record.add_date)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_many(&self, records: &[UpsertPost]) -> Result<()> {
        for record in records {
            self.upsert(record).await?;
        }
        Ok(())
    }

    /// Saves the fetched HTML body for a post.
    pub async fn save_body(&self, blog_id: &str, log_no: i64, body: &str) -> Result<()> {
        sqlx::query(
            "UPDATE posts SET body = $3, fetched_at = NOW(), updated_at = NOW() WHERE blog_id = $1 AND log_no = $2",
        )
        .bind(blog_id)
        .bind(log_no)
        .bind(body)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_unreplicated_in_categories(
        &self,
        blog_id: &str,
        category_nos: &[i32],
    ) -> Result<Vec<PostRecord>> {
        if category_nos.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query_as::<_, PostRecord>(
            r#"
            SELECT id, blog_id, log_no, title, category_no, add_date,
                   body, fetched_at, replicated_at, replication_error, created_at, updated_at
            FROM posts
            WHERE blog_id = $1
              AND category_no = ANY($2)
              AND replicated_at IS NULL
            ORDER BY log_no ASC
            "#,
        )
        .bind(blog_id)
        .bind(category_nos)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    #[allow(dead_code)]
    pub async fn mark_fetched(&self, blog_id: &str, log_no: i64) -> Result<()> {
        sqlx::query(
            "UPDATE posts SET fetched_at = NOW(), updated_at = NOW() WHERE blog_id = $1 AND log_no = $2",
        )
        .bind(blog_id)
        .bind(log_no)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_replicated(&self, blog_id: &str, log_no: i64) -> Result<()> {
        sqlx::query(
            "UPDATE posts SET replicated_at = NOW(), replication_error = NULL, updated_at = NOW() WHERE blog_id = $1 AND log_no = $2",
        )
        .bind(blog_id)
        .bind(log_no)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_replication_error(
        &self,
        blog_id: &str,
        log_no: i64,
        error: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE posts SET replication_error = $3, updated_at = NOW() WHERE blog_id = $1 AND log_no = $2",
        )
        .bind(blog_id)
        .bind(log_no)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
