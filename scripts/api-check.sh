#!/usr/bin/env bash
# Runs fmt, clippy and tests for the API inside a Rust container against the compose Postgres.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a
export MSYS_NO_PATHCONV=1
docker compose up -d db >/dev/null
docker run --rm --network fittracker_default \
  -v "$(pwd)/api:/app" -v fittrack-target:/app/target -v fittrack-cargo:/usr/local/cargo/registry \
  -w /app -e DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@db:5432/${POSTGRES_DB}" \
  rust:1 sh -c "rustup component add rustfmt clippy >/dev/null 2>&1; cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test"
