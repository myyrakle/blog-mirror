CREATE TABLE posts (
    id                SERIAL PRIMARY KEY,
    blog_id           VARCHAR(100)   NOT NULL,
    log_no            BIGINT         NOT NULL,
    title             VARCHAR(1000)  NOT NULL,
    category_no       INTEGER,
    add_date          TIMESTAMPTZ,
    fetched_at        TIMESTAMPTZ,
    replicated_at     TIMESTAMPTZ,
    replication_error TEXT,
    created_at        TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ    NOT NULL DEFAULT NOW(),
    UNIQUE (blog_id, log_no)
);

CREATE INDEX idx_posts_unreplicated ON posts (blog_id, replicated_at) WHERE replicated_at IS NULL;
CREATE INDEX idx_posts_log_no_desc  ON posts (blog_id, log_no DESC);
