-- Change time_us from TEXT to BIGINT.
-- The existing values are stored as microsecond epoch strings and cast cleanly.
ALTER TABLE posts
    ALTER COLUMN time_us TYPE BIGINT USING time_us::BIGINT;
