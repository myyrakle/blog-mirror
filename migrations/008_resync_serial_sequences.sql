-- Resync SERIAL sequences that have fallen behind their table's max(id).
--
-- A restore that inserts explicit ids leaves the sequence behind max(id).
-- Every subsequent INSERT then reuses an id that already exists, and since
-- the posts upsert conflicts on (blog_id, log_no) rather than on the primary
-- key, ON CONFLICT does not absorb it: the insert fails with
-- "duplicate key value violates unique constraint posts_pkey", which stalls
-- the sync loop indefinitely.
--
-- Only ever advances a sequence, never rewinds one.
DO $$
DECLARE
    tbl  TEXT;
    seq  TEXT;
    hi   BIGINT;
BEGIN
    FOREACH tbl IN ARRAY ARRAY['posts', 'categories', 'sync_cursor'] LOOP
        seq := pg_get_serial_sequence('public.' || tbl, 'id');
        EXECUTE format('SELECT max(id) FROM public.%I', tbl) INTO hi;
        IF hi IS NOT NULL AND hi > COALESCE(pg_sequence_last_value(seq::regclass), 0) THEN
            PERFORM setval(seq, hi);
            RAISE NOTICE 'Advanced % to %', seq, hi;
        END IF;
    END LOOP;
END $$;
