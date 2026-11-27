#!/usr/bin/env bash
# End-to-end proof that backups restore: seed a scratch DB, back it up, restore into a
# second DB, and compare row counts. Needs libpq env vars and a role that can create DBs.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
src="fittrack_restore_src_$$"; dst="fittrack_restore_dst_$$"
dir="$(mktemp -d)"
cleanup() { dropdb --if-exists "$src" --maintenance-db=postgres || true; dropdb --if-exists "$dst" --maintenance-db=postgres || true; rm -rf "$dir"; }
trap cleanup EXIT
createdb "$src" --maintenance-db=postgres
psql -v ON_ERROR_STOP=1 -q -d "$src" <<SQL
CREATE TABLE users (id serial PRIMARY KEY, email text NOT NULL UNIQUE);
CREATE TABLE sleep_logs (id serial PRIMARY KEY, user_id int REFERENCES users(id), minutes int NOT NULL);
INSERT INTO users (email) SELECT 'u' || g || '@example.com' FROM generate_series(1, 25) g;
INSERT INTO sleep_logs (user_id, minutes) SELECT (g % 25) + 1, 360 + g FROM generate_series(1, 100) g;
SQL
file="$(PGDATABASE="$src" BACKUP_DIR="$dir" bash "$here/backup.sh" | tail -1)"
bash "$here/restore.sh" "$file" "$dst"
count() { psql -At -d "$1" -c "SELECT (SELECT count(*) FROM users) || ',' || (SELECT count(*) FROM sleep_logs) || ',' || (SELECT sum(minutes) FROM sleep_logs)"; }
a="$(count "$src")"; b="$(count "$dst")"
if [ "$a" != "$b" ]; then echo "MISMATCH src=$a dst=$b" >&2; exit 1; fi
echo "restore verified ($a)"
