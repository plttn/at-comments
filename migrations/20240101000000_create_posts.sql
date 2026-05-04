CREATE TABLE IF NOT EXISTS posts (
    id      SERIAL PRIMARY KEY,
    slug    TEXT   NOT NULL UNIQUE,
    rkey    TEXT   NOT NULL,
    time_us TEXT NOT NULL
);
