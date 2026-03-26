-- Nullable custom display name for a category.
-- When set, this is used instead of the original Naver category name
-- when writing the tag into blog post frontmatter.
ALTER TABLE categories ADD COLUMN IF NOT EXISTS display_name VARCHAR(255) NULL;
