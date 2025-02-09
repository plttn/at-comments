-- This file should undo anything in `up.sql`
ALTER TABLE slug_dids
ALTER COLUMN time_us TYPE TIMESTAMP;
