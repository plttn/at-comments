-- Change time_us from TEXT to BIGINT.
-- First, replace any invalid values (like "undef") with 0.
UPDATE posts
SET time_us = '0'
WHERE time_us = 'undef' OR time_us !~ '^\d+$';

-- Now cast the column to BIGINT
ALTER TABLE posts
    ALTER COLUMN time_us TYPE BIGINT USING time_us::BIGINT;
