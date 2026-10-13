//! JSON API backing the admin dashboard.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    commands,
    db::{
        category_repo::{CategoryRepo, CategoryWithStats},
        cursor_repo::CursorRepo,
        job_repo::{JobRepo, JobRunRecord},
        post_repo::{PostFilter, PostListRow, PostRepo, PostStats, escape_like},
    },
    scheduler::{
        backfill_dates_job, replicate_job,
        resync_job::{self, ResyncRequest},
        sync_job,
    },
    web::{
        jobs::{JobKind, JobSnapshot},
        state::WebState,
    },
};

// ── Error handling ──────────────────────────────────────────────────────────

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, msg.into())
    }
    fn conflict(msg: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, msg.into())
    }
}

impl From<crate::error::AppError> for ApiError {
    /// Internal errors are logged in full but reported generically: the text
    /// can carry connection strings and other internals we don't want on the
    /// wire. Job failures still show their real cause in the job log.
    fn from(e: crate::error::AppError) -> Self {
        tracing::error!(error = %e, "api: internal error");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "서버 내부 오류가 발생했습니다. 자세한 내용은 서버 로그를 확인하세요.".to_string(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = std::result::Result<Json<T>, ApiError>;

// ── Overview ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Overview {
    blog_id: String,
    repo_path: String,
    remote_url: String,
    cursor: i64,
    posts: PostStats,
    categories: CategorySummary,
    job: Option<JobSnapshot>,
}

#[derive(Serialize)]
pub struct CategorySummary {
    total: usize,
    mirrored: usize,
}

pub async fn overview(State(st): State<Arc<WebState>>) -> ApiResult<Overview> {
    let blog_id = &st.ctx.config.naver_blog_id;

    let posts = PostRepo::new(st.ctx.pool.clone()).stats(blog_id).await?;
    let cursor = CursorRepo::new(st.ctx.pool.clone())
        .get_cursor(blog_id)
        .await?;
    let cats = CategoryRepo::new(st.ctx.pool.clone())
        .find_with_stats(blog_id)
        .await?;

    Ok(Json(Overview {
        blog_id: blog_id.clone(),
        repo_path: st.ctx.config.github_repo_path.display().to_string(),
        remote_url: st.ctx.config.github_remote_url.clone(),
        cursor,
        posts,
        categories: CategorySummary {
            total: cats.len(),
            mirrored: cats.iter().filter(|c| c.should_mirror).count(),
        },
        // Log lines are served separately by `job_status`; keep this payload small.
        job: st.jobs.snapshot(usize::MAX),
    }))
}

// ── Categories ──────────────────────────────────────────────────────────────

pub async fn list_categories(State(st): State<Arc<WebState>>) -> ApiResult<Vec<CategoryWithStats>> {
    let rows = CategoryRepo::new(st.ctx.pool.clone())
        .find_with_stats(&st.ctx.config.naver_blog_id)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct UpdateCategoryBody {
    should_mirror: Option<bool>,
    /// Present-but-null clears the display name; absent leaves it unchanged.
    #[serde(default, deserialize_with = "deserialize_double_option")]
    display_name: Option<Option<String>>,
}

/// Distinguishes `"display_name": null` (clear) from an absent key (leave as is).
fn deserialize_double_option<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

pub async fn update_category(
    State(st): State<Arc<WebState>>,
    Path(category_no): Path<i32>,
    Json(body): Json<UpdateCategoryBody>,
) -> ApiResult<serde_json::Value> {
    // Blank input means "no override", same as NULL.
    let display_name = body
        .display_name
        .map(|v| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));

    let updated = CategoryRepo::new(st.ctx.pool.clone())
        .update_settings(
            &st.ctx.config.naver_blog_id,
            category_no,
            body.should_mirror,
            display_name,
        )
        .await?;

    if !updated {
        return Err(ApiError::not_found(format!(
            "카테고리 {category_no}를 찾을 수 없습니다"
        )));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct BulkMirrorBody {
    category_nos: Vec<i32>,
    should_mirror: bool,
}

pub async fn bulk_set_mirror(
    State(st): State<Arc<WebState>>,
    Json(body): Json<BulkMirrorBody>,
) -> ApiResult<serde_json::Value> {
    let updated = CategoryRepo::new(st.ctx.pool.clone())
        .set_mirror_many(
            &st.ctx.config.naver_blog_id,
            &body.category_nos,
            body.should_mirror,
        )
        .await?;
    Ok(Json(json!({ "updated": updated })))
}

// ── Posts ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PostQuery {
    q: Option<String>,
    category_no: Option<i32>,
    status: Option<String>,
    page: Option<i64>,
    size: Option<i64>,
}

#[derive(Serialize)]
pub struct PostPage {
    items: Vec<PostListRow>,
    total: i64,
    page: i64,
    size: i64,
}

const VALID_STATUSES: [&str; 5] = ["all", "pending", "replicated", "error", "unfetched"];

pub async fn list_posts(
    State(st): State<Arc<WebState>>,
    Query(q): Query<PostQuery>,
) -> ApiResult<PostPage> {
    let status = q.status.unwrap_or_else(|| "all".to_string());
    if !VALID_STATUSES.contains(&status.as_str()) {
        return Err(ApiError::bad_request(format!(
            "알 수 없는 상태 필터: {status}"
        )));
    }

    let page = q.page.unwrap_or(1).max(1);
    let size = q.size.unwrap_or(30).clamp(1, 200);
    let query = q
        .q
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| escape_like(&s));

    let filter = PostFilter {
        query,
        category_no: q.category_no,
        status,
        limit: size,
        offset: (page - 1) * size,
    };

    let (items, total) = PostRepo::new(st.ctx.pool.clone())
        .list(&st.ctx.config.naver_blog_id, &filter)
        .await?;

    Ok(Json(PostPage {
        items,
        total,
        page,
        size,
    }))
}

/// Returns the stored Naver HTML body, for eyeballing what will be converted.
pub async fn post_body(
    State(st): State<Arc<WebState>>,
    Path(log_no): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let posts = PostRepo::new(st.ctx.pool.clone())
        .find_by_log_nos(&st.ctx.config.naver_blog_id, &[log_no])
        .await?;
    let post = posts
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found(format!("게시글 {log_no}을 찾을 수 없습니다")))?;

    let markdown = post
        .body
        .as_deref()
        .map(crate::converter::convert_html_to_markdown);

    Ok(Json(json!({
        "log_no": post.log_no,
        "title": post.title,
        "html": post.body,
        "markdown": markdown,
    })))
}

// ── Jobs ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JobLogQuery {
    /// Index of the first log line the client has not seen yet.
    from: Option<usize>,
}

#[derive(Serialize)]
pub struct JobStatusResponse {
    running: bool,
    job: Option<JobSnapshot>,
    /// Total buffered log lines, so the client knows where to resume from.
    log_len: usize,
}

pub async fn job_status(
    State(st): State<Arc<WebState>>,
    Query(q): Query<JobLogQuery>,
) -> ApiResult<JobStatusResponse> {
    Ok(Json(JobStatusResponse {
        running: st.jobs.is_running(),
        job: st.jobs.snapshot(q.from.unwrap_or(0)),
        log_len: st.jobs.log_len(),
    }))
}

pub async fn job_history(State(st): State<Arc<WebState>>) -> ApiResult<Vec<JobRunRecord>> {
    let rows = JobRepo::new(st.ctx.pool.clone())
        .recent(&st.ctx.config.naver_blog_id, 30)
        .await?;
    Ok(Json(rows))
}

pub async fn job_run_log(
    State(st): State<Arc<WebState>>,
    Path(id): Path<i32>,
) -> ApiResult<serde_json::Value> {
    let log = JobRepo::new(st.ctx.pool.clone())
        .log(&st.ctx.config.naver_blog_id, id)
        .await?;
    match log {
        Some(log) => Ok(Json(json!({ "id": id, "log": log }))),
        None => Err(ApiError::not_found("해당 작업 로그가 없습니다")),
    }
}

/// Starts one of the built-in jobs by name.
pub async fn start_job(
    State(st): State<Arc<WebState>>,
    Path(kind): Path<String>,
) -> ApiResult<JobSnapshot> {
    let kind = JobKind::parse(&kind)
        .filter(|k| *k != JobKind::Resync) // resync has its own endpoint (needs a body)
        .ok_or_else(|| ApiError::bad_request(format!("알 수 없는 작업: {kind}")))?;

    let snapshot = st
        .jobs
        .spawn(kind, move |ctx| async move {
            match kind {
                JobKind::SyncCategories => {
                    let count = commands::sync_categories::run(ctx).await?;
                    Ok(format!("카테고리 {count}개 동기화"))
                }
                JobKind::Fetch => {
                    let r = sync_job::run(ctx).await?;
                    Ok(format!("신규 게시글 {}건 수집", r.new_posts))
                }
                JobKind::Publish => {
                    let r = replicate_job::run(ctx).await?;
                    Ok(summarize_replicate(&r))
                }
                JobKind::FetchAndPublish => {
                    let f = sync_job::run(ctx.clone()).await?;
                    let r = replicate_job::run(ctx).await?;
                    Ok(format!(
                        "신규 {}건 수집 / {}",
                        f.new_posts,
                        summarize_replicate(&r)
                    ))
                }
                JobKind::BackfillDates => {
                    let r = backfill_dates_job::run(ctx, 500).await?;
                    Ok(format!(
                        "{}건 확인 / {}건 작성일 보정 / {}건 날짜 없음 / {}건 실패",
                        r.examined, r.updated, r.no_date, r.failed
                    ))
                }
                JobKind::Resync => unreachable!("filtered above"),
            }
        })
        .map_err(ApiError::conflict)?;

    Ok(Json(snapshot))
}

fn summarize_replicate(r: &replicate_job::ReplicateReport) -> String {
    if r.replicated == 0 && r.failed == 0 {
        return "복제할 게시글 없음".to_string();
    }
    let push = if r.pushed {
        "push 완료"
    } else {
        "변경사항 없음(push 생략)"
    };
    if r.failed > 0 {
        format!("{}건 복제, {}건 실패, {push}", r.replicated, r.failed)
    } else {
        format!("{}건 복제, {push}", r.replicated)
    }
}

#[derive(Deserialize)]
pub struct ResyncBody {
    #[serde(default)]
    log_nos: Vec<i64>,
    /// Re-publish every post of these categories (ignores `log_nos` when set).
    #[serde(default)]
    category_nos: Vec<i32>,
    /// Re-download bodies from Naver before republishing.
    #[serde(default)]
    refetch: bool,
}

pub async fn start_resync(
    State(st): State<Arc<WebState>>,
    Json(body): Json<ResyncBody>,
) -> ApiResult<JobSnapshot> {
    let blog_id = st.ctx.config.naver_blog_id.clone();

    // Category-wide resync is expanded into an explicit log_no list up front so
    // the job reports exactly what it touched.
    let mut log_nos = body.log_nos;
    if !body.category_nos.is_empty() {
        let from_categories = PostRepo::new(st.ctx.pool.clone())
            .find_log_nos_by_categories(&blog_id, &body.category_nos)
            .await?;
        log_nos.extend(from_categories);
    }

    log_nos.sort_unstable();
    log_nos.dedup();

    if log_nos.is_empty() {
        return Err(ApiError::bad_request("재동기화할 게시글이 없습니다"));
    }
    if log_nos.len() > 5000 {
        return Err(ApiError::bad_request(
            "한 번에 5000건을 초과해 재동기화할 수 없습니다",
        ));
    }

    let refetch = body.refetch;
    let snapshot = st
        .jobs
        .spawn(JobKind::Resync, move |ctx| async move {
            let report = resync_job::run(ctx, ResyncRequest { log_nos, refetch }).await?;
            let mut parts = vec![format!("{}건 요청", report.requested)];
            if refetch {
                parts.push(format!("{}건 본문 재수집", report.refetched));
                if report.refetch_failed > 0 {
                    parts.push(format!("{}건 재수집 실패", report.refetch_failed));
                }
            }
            parts.push(summarize_replicate(&report.replicate));
            if !report.skipped_not_mirrored.is_empty() {
                parts.push(format!(
                    "{}건은 복제 대상 카테고리가 아니어서 게시되지 않음",
                    report.skipped_not_mirrored.len()
                ));
            }
            Ok(parts.join(" / "))
        })
        .map_err(ApiError::conflict)?;

    Ok(Json(snapshot))
}
