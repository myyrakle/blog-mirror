CREATE TABLE sync_cursor (
    id          SERIAL PRIMARY KEY,
    blog_id     VARCHAR(100)  NOT NULL UNIQUE,
    last_log_no BIGINT        NOT NULL DEFAULT 0,
    updated_at  TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);
