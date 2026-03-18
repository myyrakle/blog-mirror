CREATE TABLE categories (
    id            SERIAL PRIMARY KEY,
    blog_id       VARCHAR(100)  NOT NULL,
    category_no   INTEGER       NOT NULL,
    parent_no     INTEGER,
    name          VARCHAR(255)  NOT NULL,
    post_count    INTEGER       NOT NULL DEFAULT 0,
    should_mirror BOOLEAN       NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    UNIQUE (blog_id, category_no)
);
