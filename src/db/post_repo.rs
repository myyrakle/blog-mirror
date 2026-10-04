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

    // ── Admin dashboard queries ─────────────────────────────────────────────

    /// Aggregate counters shown on the dashboard overview.
    pub async fn stats(&self, blog_id: &str) -> Result<PostStats> {
        let row = sqlx::query_as::<_, PostStats>(
            r#"
            SELECT COUNT(*)                                                    AS total,
                   COUNT(body)                                                 AS fetched,
                   COUNT(replicated_at)                                        AS replicated,
                   COUNT(*) FILTER (WHERE replication_error IS NOT NULL)       AS errors,
                   MAX(add_date)                                               AS latest_post_at,
                   MAX(replicated_at)                                          AS last_replicated_at
            FROM posts
            WHERE blog_id = $1
            "#,
        )
        .bind(blog_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Paginated post listing with optional text / category / status filters.
    /// `status` is one of: all, pending, replicated, error, unfetched.
    pub async fn list(&self, blog_id: &str, filter: &PostFilter) -> Result<(Vec<PostListRow>, i64)> {
        // Shared WHERE clause. Every branch uses the same bind order so the
        // count query and the page query can be built from one predicate.
        let where_sql = r#"
            WHERE p.blog_id = $1
              AND ($2::int  IS NULL OR p.category_no = $2::int)
              AND ($3::text IS NULL OR p.title ILIKE '%' || $3::text || '%'
                                    OR CAST(p.log_no AS TEXT) LIKE '%' || $3::text || '%')
              AND ($4::text = 'all'
                   OR ($4::text = 'pending'    AND p.replicated_at IS NULL)
                   OR ($4::text = 'replicated' AND p.replicated_at IS NOT NULL)
                   OR ($4::text = 'error'      AND p.replication_error IS NOT NULL)
                   OR ($4::text = 'unfetched'  AND p.body IS NULL))
        "#;

        let count_sql = format!("SELECT COUNT(*) FROM posts p {where_sql}");
        let (total,): (i64,) = sqlx::query_as(&count_sql)
            .bind(blog_id)
            .bind(filter.category_no)
            .bind(filter.query.as_deref())
            .bind(filter.status.as_str())
            .fetch_one(&self.pool)
            .await?;

        let list_sql = format!(
            r#"
            SELECT p.log_no,
                   p.title,
                   p.category_no,
                   c.name                                  AS category_name,
                   c.display_name                          AS category_display_name,
                   COALESCE(c.should_mirror, FALSE)        AS category_mirrored,
                   p.add_date,
                   p.fetched_at,
                   p.replicated_at,
                   p.replication_error,
                   (p.body IS NOT NULL)                    AS has_body,
                   p.updated_at
            FROM posts p
            LEFT JOIN categories c
                   ON c.blog_id = p.blog_id AND c.category_no = p.category_no
            {where_sql}
            ORDER BY p.log_no DESC
            LIMIT $5 OFFSET $6
            "#
        );
        let rows = sqlx::query_as::<_, PostListRow>(&list_sql)
            .bind(blog_id)
            .bind(filter.category_no)
            .bind(filter.query.as_deref())
            .bind(filter.status.as_str())
            .bind(filter.limit)
            .bind(filter.offset)
            .fetch_all(&self.pool)
            .await?;

        Ok((rows, total))
    }

    pub async fn find_by_log_nos(&self, blog_id: &str, log_nos: &[i64]) -> Result<Vec<PostRecord>> {
        if log_nos.is_empty() {
            return Ok(vec![]);
        }
        let rows = sqlx::query_as::<_, PostRecord>(
            r#"
            SELECT id, blog_id, log_no, title, category_no, add_date,
                   body, fetched_at, replicated_at, replication_error, created_at, updated_at
            FROM posts
            WHERE blog_id = $1 AND log_no = ANY($2)
            ORDER BY log_no ASC
            "#,
        )
        .bind(blog_id)
        .bind(log_nos)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Clears replication state so the next publish run rewrites these posts.
    pub async fn reset_replication(&self, blog_id: &str, log_nos: &[i64]) -> Result<u64> {
        if log_nos.is_empty() {
            return Ok(0);
        }
        let result = sqlx::query(
            "UPDATE posts SET replicated_at = NULL, replication_error = NULL, updated_at = NOW()
             WHERE blog_id = $1 AND log_no = ANY($2)",
        )
        .bind(blog_id)
        .bind(log_nos)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// All log_nos belonging to the given categories, for category-wide re-sync.
    pub async fn find_log_nos_by_categories(
        &self,
        blog_id: &str,
        category_nos: &[i32],
    ) -> Result<Vec<i64>> {
        if category_nos.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT log_no FROM posts WHERE blog_id = $1 AND category_no = ANY($2) ORDER BY log_no",
        )
        .bind(blog_id)
        .bind(category_nos)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }
}

/// Aggregate post counters for the dashboard.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PostStats {
    pub total: i64,
    pub fetched: i64,
    pub replicated: i64,
    pub errors: i64,
    pub latest_post_at: Option<DateTime<Utc>>,
    pub last_replicated_at: Option<DateTime<Utc>>,
}

/// One row of the admin post list.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PostListRow {
    pub log_no: i64,
    pub title: String,
    pub category_no: Option<i32>,
    pub category_name: Option<String>,
    pub category_display_name: Option<String>,
    pub category_mirrored: bool,
    pub add_date: Option<DateTime<Utc>>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub replicated_at: Option<DateTime<Utc>>,
    pub replication_error: Option<String>,
    pub has_body: bool,
    pub updated_at: DateTime<Utc>,
}

/// Filter + pagination parameters for [`PostRepo::list`].
#[derive(Debug)]
pub struct PostFilter {
    pub query: Option<String>,
    pub category_no: Option<i32>,
    pub status: String,
    pub limit: i64,
    pub offset: i64,
}
