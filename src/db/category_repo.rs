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
    pub display_name: Option<String>,
    pub post_count: i32,
    pub should_mirror: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CategoryRecord {
    /// Returns display_name if set, otherwise falls back to name.
    pub fn effective_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

pub struct CategoryRepo {
    pool: PgPool,
}

impl CategoryRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Full upsert: inserts or updates name, parent_no, post_count.
    /// Use this when fetching from the category API (real names).
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

    /// Insert only if not exists: preserves existing name when called from post list fallback.
    pub async fn insert_if_not_exists(&self, record: &UpsertCategory) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO categories (blog_id, category_no, parent_no, name, post_count, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (blog_id, category_no) DO NOTHING
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

    pub async fn insert_many_if_not_exists(&self, records: &[UpsertCategory]) -> Result<()> {
        for record in records {
            self.insert_if_not_exists(record).await?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn find_by_blog_id(&self, blog_id: &str) -> Result<Vec<CategoryRecord>> {
        let rows = sqlx::query_as::<_, CategoryRecord>(
            "SELECT id, blog_id, category_no, parent_no, name, display_name, post_count, should_mirror, created_at, updated_at
             FROM categories WHERE blog_id = $1 ORDER BY category_no",
        )
        .bind(blog_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Categories joined with post counts from the local DB, for the admin dashboard.
    pub async fn find_with_stats(&self, blog_id: &str) -> Result<Vec<CategoryWithStats>> {
        let rows = sqlx::query_as::<_, CategoryWithStats>(
            r#"
            SELECT c.category_no,
                   c.parent_no,
                   c.name,
                   c.display_name,
                   c.post_count,
                   c.should_mirror,
                   COALESCE(p.total, 0)      AS db_post_count,
                   COALESCE(p.replicated, 0) AS replicated_count,
                   COALESCE(p.errors, 0)     AS error_count
            FROM categories c
            LEFT JOIN (
                SELECT category_no,
                       COUNT(*)                                        AS total,
                       COUNT(replicated_at)                            AS replicated,
                       COUNT(*) FILTER (WHERE replication_error IS NOT NULL) AS errors
                FROM posts
                WHERE blog_id = $1
                GROUP BY category_no
            ) p ON p.category_no = c.category_no
            WHERE c.blog_id = $1
            ORDER BY c.category_no
            "#,
        )
        .bind(blog_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Updates the admin-editable fields of a category.
    /// `None` leaves a field untouched; `Some(None)` for display_name clears it.
    pub async fn update_settings(
        &self,
        blog_id: &str,
        category_no: i32,
        should_mirror: Option<bool>,
        display_name: Option<Option<String>>,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE categories
            SET should_mirror = COALESCE($3::bool, should_mirror),
                display_name  = CASE WHEN $4::bool THEN $5::varchar ELSE display_name END,
                updated_at    = NOW()
            WHERE blog_id = $1 AND category_no = $2
            "#,
        )
        .bind(blog_id)
        .bind(category_no)
        .bind(should_mirror)
        .bind(display_name.is_some())
        .bind(display_name.flatten())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Bulk toggles `should_mirror` for several categories at once.
    pub async fn set_mirror_many(
        &self,
        blog_id: &str,
        category_nos: &[i32],
        should_mirror: bool,
    ) -> Result<u64> {
        if category_nos.is_empty() {
            return Ok(0);
        }
        let result = sqlx::query(
            "UPDATE categories SET should_mirror = $3, updated_at = NOW()
             WHERE blog_id = $1 AND category_no = ANY($2)",
        )
        .bind(blog_id)
        .bind(category_nos)
        .bind(should_mirror)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn find_mirror_categories(&self, blog_id: &str) -> Result<Vec<CategoryRecord>> {
        let rows = sqlx::query_as::<_, CategoryRecord>(
            "SELECT id, blog_id, category_no, parent_no, name, display_name, post_count, should_mirror, created_at, updated_at
             FROM categories WHERE blog_id = $1 AND should_mirror = TRUE ORDER BY category_no",
        )
        .bind(blog_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

/// A category row enriched with locally stored post counts.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CategoryWithStats {
    pub category_no: i32,
    pub parent_no: Option<i32>,
    pub name: String,
    pub display_name: Option<String>,
    /// Post count reported by Naver
    pub post_count: i32,
    pub should_mirror: bool,
    /// Posts stored in our own DB for this category
    pub db_post_count: i64,
    pub replicated_count: i64,
    pub error_count: i64,
}

#[derive(Debug)]
pub struct UpsertCategory {
    pub blog_id: String,
    pub category_no: i32,
    pub parent_no: Option<i32>,
    pub name: String,
    pub post_count: i32,
}
