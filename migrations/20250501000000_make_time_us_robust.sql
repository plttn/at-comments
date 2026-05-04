-- Safely convert posts.time_us to BIGINT if needed.
-- This migration is idempotent: it checks the column type first and only runs
-- the ALTER if the column exists and is not already bigint.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'posts'
      AND column_name = 'time_us'
      AND data_type <> 'bigint'
  ) THEN
    -- Convert numeric strings to bigint; non-numeric values become 0.
    ALTER TABLE posts
      ALTER COLUMN time_us TYPE BIGINT USING (
        CASE
          WHEN time_us ~ '^[0-9]+$' THEN time_us::bigint
          ELSE 0
        END
      );
  END IF;
END
$$;
