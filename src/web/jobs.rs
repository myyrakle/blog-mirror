//! Background job runner for the admin dashboard.
//!
//! Only one job may run at a time (the underlying jobs hit Naver and push to
//! git, so overlapping runs would fight each other). While a job runs, every
//! `blog_mirror` tracing event is mirrored into an in-memory buffer so the UI
//! can stream the live log; when the job finishes the buffer is persisted to
//! the `job_runs` table.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::layer::{Context as LayerContext, Layer};

use crate::{context::AppContext, db::job_repo::JobRepo};

const MAX_LOG_LINES: usize = 3000;

// ── Live log capture ────────────────────────────────────────────────────────

type LogSink = Arc<Mutex<Vec<String>>>;

static LOG_SINK: Mutex<Option<LogSink>> = Mutex::new(None);

fn set_log_sink(sink: Option<LogSink>) {
    if let Ok(mut guard) = LOG_SINK.lock() {
        *guard = sink;
    }
}

fn current_log_sink() -> Option<LogSink> {
    LOG_SINK.lock().ok().and_then(|g| g.clone())
}

/// Tracing layer that tees `blog_mirror` events into the running job's log buffer.
pub struct JobLogLayer;

impl<S: Subscriber> Layer<S> for JobLogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        if !event.metadata().target().starts_with("blog_mirror") {
            return;
        }
        let Some(sink) = current_log_sink() else {
            return;
        };

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let line = format!(
            "[{}] {:>5} {}{}",
            Utc::now().format("%H:%M:%S"),
            event.metadata().level(),
            visitor.message,
            visitor.fields
        );

        if let Ok(mut buf) = sink.lock() {
            if buf.len() >= MAX_LOG_LINES {
                let overflow = buf.len() - MAX_LOG_LINES + 1;
                buf.drain(..overflow);
            }
            buf.push(line);
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    fields: String,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .push_str(&format!(" {}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push_str(&format!(" {}={}", field.name(), value));
        }
    }
}

// ── Job model ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobKind {
    SyncCategories,
    Fetch,
    Publish,
    FetchAndPublish,
    Resync,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::SyncCategories => "sync-categories",
            JobKind::Fetch => "fetch",
            JobKind::Publish => "publish",
            JobKind::FetchAndPublish => "fetch-and-publish",
            JobKind::Resync => "resync",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            JobKind::SyncCategories => "카테고리 동기화",
            JobKind::Fetch => "게시글 수집",
            JobKind::Publish => "GitHub 복제",
            JobKind::FetchAndPublish => "수집 + 복제",
            JobKind::Resync => "재동기화",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sync-categories" => Some(JobKind::SyncCategories),
            "fetch" => Some(JobKind::Fetch),
            "publish" => Some(JobKind::Publish),
            "fetch-and-publish" => Some(JobKind::FetchAndPublish),
            "resync" => Some(JobKind::Resync),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Success,
    Failed,
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            JobStatus::Running => "running",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
        }
    }
}

/// Serializable view of the current (or most recent) job.
#[derive(Debug, Clone, Serialize)]
pub struct JobSnapshot {
    pub id: i64,
    pub kind: JobKind,
    pub label: &'static str,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
    pub logs: Vec<String>,
}

struct JobRecord {
    id: i64,
    kind: JobKind,
    status: JobStatus,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    message: Option<String>,
    logs: LogSink,
}

impl JobRecord {
    fn snapshot(&self, from_line: usize) -> JobSnapshot {
        let logs = self
            .logs
            .lock()
            .map(|b| b.get(from_line..).unwrap_or(&[]).to_vec())
            .unwrap_or_default();

        JobSnapshot {
            id: self.id,
            kind: self.kind,
            label: self.kind.label(),
            status: self.status,
            started_at: self.started_at,
            finished_at: self.finished_at,
            message: self.message.clone(),
            logs,
        }
    }
}

// ── Manager ─────────────────────────────────────────────────────────────────

pub struct JobManager {
    ctx: Arc<AppContext>,
    current: Mutex<Option<JobRecord>>,
    next_id: AtomicI64,
}

impl JobManager {
    pub fn new(ctx: Arc<AppContext>) -> Arc<Self> {
        Arc::new(Self {
            ctx,
            current: Mutex::new(None),
            next_id: AtomicI64::new(1),
        })
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.current.lock().ok().and_then(|g| g.as_ref().map(|j| j.status)),
            Some(JobStatus::Running)
        )
    }

    /// Current/last job, including log lines from index `from_line` onward.
    pub fn snapshot(&self, from_line: usize) -> Option<JobSnapshot> {
        self.current
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|j| j.snapshot(from_line)))
    }

    /// Total number of log lines buffered for the current/last job.
    pub fn log_len(&self) -> usize {
        self.current
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|j| j.logs.lock().map(|b| b.len()).unwrap_or(0)))
            .unwrap_or(0)
    }

    /// Spawns `task` as the single active job.
    /// Returns `Err` if another job is already running.
    pub fn spawn<F, Fut>(
        self: &Arc<Self>,
        kind: JobKind,
        task: F,
    ) -> std::result::Result<JobSnapshot, String>
    where
        F: FnOnce(Arc<AppContext>) -> Fut + Send + 'static,
        Fut: Future<Output = crate::error::Result<String>> + Send + 'static,
    {
        let logs: LogSink = Arc::new(Mutex::new(Vec::new()));

        let snapshot = {
            let mut guard = self
                .current
                .lock()
                .map_err(|_| "작업 상태 잠금에 실패했습니다".to_string())?;

            if let Some(job) = guard.as_ref()
                && job.status == JobStatus::Running
            {
                return Err(format!("이미 '{}' 작업이 실행 중입니다", job.kind.label()));
            }

            let record = JobRecord {
                id: self.next_id.fetch_add(1, Ordering::SeqCst),
                kind,
                status: JobStatus::Running,
                started_at: Utc::now(),
                finished_at: None,
                message: None,
                logs: logs.clone(),
            };
            let snapshot = record.snapshot(0);
            *guard = Some(record);
            snapshot
        };

        set_log_sink(Some(logs.clone()));

        let manager = self.clone();
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            let job_repo = JobRepo::new(ctx.pool.clone());
            let blog_id = ctx.config.naver_blog_id.clone();
            let run_id = match job_repo.start(&blog_id, kind.as_str()).await {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(error = %e, "job: failed to record job start");
                    None
                }
            };

            tracing::info!(job = kind.as_str(), "job: started");
            let outcome = task(ctx).await;

            let (status, message) = match &outcome {
                Ok(msg) => {
                    tracing::info!(job = kind.as_str(), summary = %msg, "job: finished");
                    (JobStatus::Success, msg.clone())
                }
                Err(e) => {
                    tracing::error!(job = kind.as_str(), error = %e, "job: failed");
                    (JobStatus::Failed, e.to_string())
                }
            };

            let captured = logs.lock().map(|b| b.join("\n")).unwrap_or_default();
            set_log_sink(None);

            if let Ok(mut guard) = manager.current.lock()
                && let Some(job) = guard.as_mut()
            {
                job.status = status;
                job.finished_at = Some(Utc::now());
                job.message = Some(message.clone());
            }

            if let Some(id) = run_id
                && let Err(e) = job_repo
                    .finish(id, status.as_str(), &message, &captured)
                    .await
            {
                tracing::warn!(error = %e, "job: failed to record job result");
            }
        });

        Ok(snapshot)
    }
}
