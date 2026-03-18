use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct CategoryRecord {
    pub id: i32,
    pub blog_id: String,
    pub category_no: i32,
    pub parent_no: Option<i32>,
    pub name: String,
    pub post_count: i32,
    pub should_mirror: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct CategoryRepo {
    pool: PgPool,
}

impl CategoryRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, record: &UpsertCategory) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO categories (blog_id, category_no, parent_no, name, post_count, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (blog_id, category_no)
            DO UPDATE SET
                name       = EXCLUDED.name,
                parent_no  = EXCLUDED.parent_no,
                post_count = EXCLUDED.post_count,
                updated_at = NOW()
            "#,
        )
        .bind(&record.blog_id)
        .bind(record.category_no)
        .bind(record.parent_no)
        .bind(&record.name)
        .bind(record.post_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_many(&self, records: &[UpsertCategory]) -> Result<()> {
        for record in records {
            self.upsert(record).await?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn find_by_blog_id(&self, blog_id: &str) -> Result<Vec<CategoryRecord>> {
        let rows = sqlx::query_as::<_, CategoryRecord>(
            "SELECT id, blog_id, category_no, parent_no, name, post_count, should_mirror, created_at, updated_at
             FROM categories WHERE blog_id = $1 ORDER BY category_no",
        )
        .bind(blog_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_mirror_categories(&self, blog_id: &str) -> Result<Vec<CategoryRecord>> {
        let rows = sqlx::query_as::<_, CategoryRecord>(
            "SELECT id, blog_id, category_no, parent_no, name, post_count, should_mirror, created_at, updated_at
             FROM categories WHERE blog_id = $1 AND should_mirror = TRUE ORDER BY category_no",
        )
        .bind(blog_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug)]
pub struct UpsertCategory {
    pub blog_id: String,
    pub category_no: i32,
    pub parent_no: Option<i32>,
    pub name: String,
    pub post_count: i32,
}
