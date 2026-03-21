-- Decode common HTML entities in post titles that were stored without decoding.
-- Handles: &amp; &lt; &gt; &quot; &apos; &nbsp; and decimal numeric entities like &#39;

CREATE OR REPLACE FUNCTION decode_html_entities(input TEXT) RETURNS TEXT AS $$
DECLARE
    result TEXT := input;
BEGIN
    -- Named entities
    result := replace(result, '&amp;',  '&');
    result := replace(result, '&lt;',   '<');
    result := replace(result, '&gt;',   '>');
    result := replace(result, '&quot;', '"');
    result := replace(result, '&apos;', '''');
    result := replace(result, '&nbsp;', ' ');
    result := replace(result, '&copy;', '©');
    result := replace(result, '&reg;',  '®');

    -- Common decimal numeric entities
    result := replace(result, '&#39;',  '''');
    result := replace(result, '&#34;',  '"');
    result := replace(result, '&#38;',  '&');
    result := replace(result, '&#60;',  '<');
    result := replace(result, '&#62;',  '>');
    result := replace(result, '&#160;', ' ');

    RETURN result;
END;
$$ LANGUAGE plpgsql;

UPDATE posts
SET title = decode_html_entities(title)
WHERE title ~ '&[a-zA-Z#][^;]{1,10};';

DROP FUNCTION decode_html_entities;
