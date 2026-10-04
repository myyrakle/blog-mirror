-- History of admin-triggered (and scheduled) jobs, shown in the web dashboard.
CREATE TABLE IF NOT EXISTS job_runs (
    id          SERIAL PRIMARY KEY,
    blog_id     VARCHAR(100) NOT NULL,
    kind        VARCHAR(50)  NOT NULL,
    status      VARCHAR(20)  NOT NULL,
    message     TEXT,
    log         TEXT,
    started_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_job_runs_started ON job_runs (blog_id, started_at DESC);
