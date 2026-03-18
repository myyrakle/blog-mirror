use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Scheduler error: {0}")]
    Scheduler(#[from] tokio_cron_scheduler::JobSchedulerError),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

pub type Result<T> = std::result::Result<T, AppError>;
