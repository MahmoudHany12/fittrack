#!/usr/bin/env bash
# Daily Postgres backup. Uses libpq env vars (PGHOST, PGPORT, PGUSER, PGPASSWORD, PGDATABASE).
# Optional: AGE_RECIPIENT=<age public key> encrypts the dump (health data at rest).
# BACKUP_DIR (default ./backups), RETENTION_DAYS (default 14).
set -euo pipefail
BACKUP_DIR="${BACKUP_DIR:-./backups}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"
mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="$BACKUP_DIR/fittrack-$stamp.dump"
umask 077
pg_dump --format=custom --no-owner --no-privileges --file="$out"
if [ -n "${AGE_RECIPIENT:-}" ]; then
  age -r "$AGE_RECIPIENT" -o "$out.age" "$out"
  rm -f "$out"
  out="$out.age"
fi
# Sanity: a non-empty file
[ -s "$out" ] || { echo "backup is empty" >&2; exit 1; }
find "$BACKUP_DIR" -name 'fittrack-*.dump*' -mtime "+$RETENTION_DAYS" -delete
echo "$out"
