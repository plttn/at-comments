#!/usr/bin/env bash
set -euo pipefail

CONTAINER_NAME="${CONTAINER_NAME:-at-comments-postgres}"
POSTGRES_DB="${POSTGRES_DB:-at_comments}"
POSTGRES_USER="${POSTGRES_USER:-at_comments}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-at_comments}"
POSTGRES_PORT="${POSTGRES_PORT:-54329}"
SEED_DATA="${SEED_DATA:-1}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required but not installed or not in PATH"
  exit 1
fi

container_exists() {
  docker ps -a --format '{{.Names}}' | grep -qx "$CONTAINER_NAME"
}

container_running() {
  [ "$(docker inspect -f '{{.State.Running}}' "$CONTAINER_NAME" 2>/dev/null || true)" = "true" ]
}

if container_exists; then
  if container_running; then
    echo "Container '$CONTAINER_NAME' is already running"
  else
    echo "Starting existing container '$CONTAINER_NAME'"
    docker start "$CONTAINER_NAME" >/dev/null
  fi
else
  echo "Creating container '$CONTAINER_NAME' on localhost:$POSTGRES_PORT"
  docker run -d \
    --name "$CONTAINER_NAME" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    -p "$POSTGRES_PORT:5432" \
    postgres:16-alpine >/dev/null
fi

echo "Waiting for Postgres to become ready..."
for _ in $(seq 1 45); do
  if docker exec "$CONTAINER_NAME" pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! docker exec "$CONTAINER_NAME" pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
  echo "Postgres did not become ready in time"
  exit 1
fi

echo "Creating schema (no migrations)..."
docker exec -i "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" <<'SQL'
CREATE TABLE IF NOT EXISTS posts (
  id SERIAL PRIMARY KEY,
  slug TEXT UNIQUE NOT NULL,
  rkey TEXT NOT NULL,
  time_us TEXT NOT NULL
);
SQL

if [ "$SEED_DATA" = "1" ]; then
  echo "Seeding sample rows..."
  docker exec -i "$CONTAINER_NAME" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" <<'SQL'
INSERT INTO posts (slug, rkey, time_us) VALUES
  ('hello-world', '3lch7i6x4x22m', '1738411000000000'),
  ('rust-and-rockets', '3lch7i6x4x22n', '1738414600000000'),
  ('using-bluesky-comments', '3lch7i6x4x22o', '1738418200000000')
ON CONFLICT (slug) DO UPDATE SET
  rkey = EXCLUDED.rkey,
  time_us = EXCLUDED.time_us;
SQL
fi

echo
echo "Local Postgres is ready."
DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:${POSTGRES_PORT}/${POSTGRES_DB}"
printf "Run app with:\n"
printf "ROCKET_DATABASES='{bluesky_comments={url=\"%s\"}}' cargo run\n" "$DATABASE_URL"
printf "\nQuick test:\n"
printf "curl http://127.0.0.1:4321/slug/hello-world\n"
printf "\nTo stop/remove DB container later:\n"
printf "docker rm -f %s\n" "$CONTAINER_NAME"
