#!/usr/bin/env bash
# Restore a backup into a database. Usage: restore.sh <dump-file> <target-db>
# For .age files set AGE_IDENTITY=<path to private key file>.
# Uses libpq env vars. The target database is dropped and recreated.
set -euo pipefail
file="${1:?usage: restore.sh <dump-file> <target-db>}"
db="${2:?usage: restore.sh <dump-file> <target-db>}"
tmp=""
cleanup() { [ -n "$tmp" ] && rm -f "$tmp"; return 0; }
trap cleanup EXIT
if [[ "$file" == *.age ]]; then
  : "${AGE_IDENTITY:?set AGE_IDENTITY for encrypted backups}"
  tmp="$(mktemp)"
  age -d -i "$AGE_IDENTITY" -o "$tmp" "$file"
  file="$tmp"
fi
dropdb --if-exists "$db" --maintenance-db=postgres
createdb "$db" --maintenance-db=postgres
pg_restore --no-owner --no-privileges --exit-on-error --dbname="$db" "$file"
echo "restored into $db"
