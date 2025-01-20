-- Your SQL goes here
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    post_did TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);