-- Aquarium device request (43200) and receipt (43201) are NIP-44 ciphertext
-- addressed via `#p`. Exclude them from full-text search without changing the
-- search policy of existing installations. Fresh installs are already safe via
-- the positive allowlist from migration 0008; this closes the brownfield gap
-- where a populated database still runs the legacy negative skip-set
-- (0001/0005/0014/0033) and would tokenize the ciphertext into search_tsv.
--
-- Same shape as 0014 (kind:30350) and 0033 (kind:30179): PostgreSQL cannot
-- alter a generated expression in place, so capture the current expression,
-- drop the column, and re-add it wrapped with the new exclusion. Every other
-- kind keeps whatever policy the database had before.
--
-- Operational cost: DROP COLUMN + ADD ... GENERATED ... STORED rewrites the
-- events heap and rebuilds the GIN index under ACCESS EXCLUSIVE. Isolated
-- demo databases are small; operators with large brownfield tables should
-- schedule a window.
DO $$
DECLARE
    existing_expression TEXT;
BEGIN
    SELECT pg_get_expr(d.adbin, d.adrelid)
      INTO existing_expression
      FROM pg_attrdef d
      JOIN pg_attribute a
        ON a.attrelid = d.adrelid
       AND a.attnum = d.adnum
     WHERE d.adrelid = 'events'::regclass
       AND a.attname = 'search_tsv';

    IF existing_expression IS NULL THEN
        RAISE EXCEPTION 'events.search_tsv generated expression not found';
    END IF;

    ALTER TABLE events DROP COLUMN search_tsv;
    EXECUTE format(
        'ALTER TABLE events ADD COLUMN search_tsv TSVECTOR GENERATED ALWAYS AS (CASE WHEN kind IN (43200, 43201) THEN NULL::tsvector ELSE (%s) END) STORED',
        existing_expression
    );
    CREATE INDEX idx_events_search_tsv ON events USING GIN (search_tsv);
END $$;
