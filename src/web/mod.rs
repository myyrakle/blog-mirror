//! Admin dashboard: a small HTTP server exposing category management,
//! manual sync triggers and post re-synchronisation.

pub mod api;
pub mod jobs;
pub mod state;

use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, patch, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::{
    context::AppContext,
    db::{job_repo::JobRepo, run_migrations},
    error::Result,
    web::{jobs::JobManager, state::WebState},
};

const INDEX_HTML: &str = include_str!("assets/index.html");
const APP_CSS: &str = include_str!("assets/app.css");
const APP_JS: &str = include_str!("assets/app.js");

/// Starts the admin web server and blocks until shutdown.
///
/// When `interval_secs` is non-zero the server also runs `fetch` + `publish`
/// on that interval, going through the same job manager as the manual
/// buttons — so a scheduled run and a dashboard-triggered run can never
/// touch the git checkout at the same time.
pub async fn serve(ctx: Arc<AppContext>, port: u16, interval_secs: u64) -> Result<()> {
    run_migrations(&ctx.pool).await?;
    info!("Migrations complete");

    // Runs left over from a previous process can never finish; don't show them as live.
    JobRepo::new(ctx.pool.clone())
        .mark_orphans_interrupted(&ctx.config.naver_blog_id)
        .await?;

    let auth = ctx.config.web_credentials()?;
    if auth.is_none() {
        warn!("WEB_USERNAME/WEB_PASSWORD not set — the dashboard is unauthenticated");
    }

    let jobs = JobManager::new(ctx.clone());
    if interval_secs > 0 {
        spawn_scheduled_sync(jobs.clone(), interval_secs);
    } else {
        info!("Scheduled sync disabled (--interval 0); use the dashboard buttons");
    }

    let state = Arc::new(WebState { ctx, jobs, auth });

    let app = router(state.clone());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "Admin dashboard listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            info!("Shutting down admin dashboard");
        })
        .await?;

    Ok(())
}

/// Periodically enqueues a fetch + publish run.
/// A tick that lands while another job is running is skipped, not queued;
/// the next tick picks it up.
fn spawn_scheduled_sync(jobs: Arc<JobManager>, interval_secs: u64) {
    info!(interval_secs, "Scheduled sync enabled");
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            let result = jobs.spawn(jobs::JobKind::FetchAndPublish, |ctx| async move {
                let fetched = crate::scheduler::sync_job::run(ctx.clone()).await?;
                let replicated = crate::scheduler::replicate_job::run(ctx).await?;
                Ok(format!(
                    "신규 {}건 수집 / {}건 복제",
                    fetched.new_posts, replicated.replicated
                ))
            });
            if let Err(e) = result {
                info!(reason = %e, "Scheduled sync skipped this tick");
            }
        }
    });
}

fn router(state: Arc<WebState>) -> Router {
    let api = Router::new()
        .route("/overview", get(api::overview))
        .route("/categories", get(api::list_categories))
        .route("/categories/{category_no}", patch(api::update_category))
        .route("/categories/mirror", post(api::bulk_set_mirror))
        .route("/posts", get(api::list_posts))
        .route("/posts/{log_no}/body", get(api::post_body))
        .route("/jobs/status", get(api::job_status))
        .route("/jobs/history", get(api::job_history))
        .route("/jobs/history/{id}/log", get(api::job_run_log))
        .route("/jobs/resync", post(api::start_resync))
        .route("/jobs/{kind}", post(api::start_job));

    Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api)
        .layer(middleware::from_fn(require_json_for_writes))
        .layer(middleware::from_fn_with_state(state.clone(), basic_auth))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_css() -> Response {
    static_asset("text/css; charset=utf-8", APP_CSS)
}

async fn app_js() -> Response {
    static_asset("text/javascript; charset=utf-8", APP_JS)
}

fn static_asset(content_type: &'static str, body: &'static str) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        body,
    )
        .into_response()
}

/// Rejects state-changing requests that a plain HTML form could have sent.
///
/// `POST /api/jobs/{kind}` takes no body, so without this an attacker's page
/// could start a crawl or a publish through a cross-origin form submit, riding
/// on the browser's cached Basic credentials. Forms can only send
/// `application/x-www-form-urlencoded`, `multipart/form-data` or `text/plain`,
/// so requiring JSON is enough to block them while leaving `fetch` working.
async fn require_json_for_writes(req: Request, next: Next) -> Response {
    let is_write = !matches!(req.method(), &axum::http::Method::GET | &axum::http::Method::HEAD);

    if is_write {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !content_type
            .split(';')
            .next()
            .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
        {
            return (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                axum::Json(serde_json::json!({
                    "error": "Content-Type: application/json 이 필요합니다"
                })),
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// HTTP Basic auth, skipped entirely when no credentials are configured.
async fn basic_auth(State(state): State<Arc<WebState>>, req: Request, next: Next) -> Response {
    let Some((user, pass)) = state.auth.as_ref() else {
        return next.run(req).await;
    };

    // Health checks must stay reachable for k8s probes.
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }

    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| BASE64.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());

    let expected = format!("{user}:{pass}");
    if provided.is_some_and(|got| constant_time_eq(&got, &expected)) {
        return next.run(req).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"blog-mirror\""),
        )],
        "unauthorized",
    )
        .into_response()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
