-- This file should undo anything in `up.sql`
ALTER TABLE posts RENAME COLUMN rkey TO post_did;