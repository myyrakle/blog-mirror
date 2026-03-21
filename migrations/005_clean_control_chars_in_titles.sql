-- Strip control characters (U+0000–U+0008, U+000B–U+001F, U+007F) from post titles.
-- These can come from JSON \b and similar escape sequences in Naver's API responses
-- and cause TOML parse errors in Zola frontmatter.
UPDATE posts
SET title = regexp_replace(title, '[\x00-\x08\x0b-\x1f\x7f]', '', 'g')
WHERE title ~ '[\x00-\x08\x0b-\x1f\x7f]';
