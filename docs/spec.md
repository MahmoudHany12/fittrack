# FitTrack spec (source of truth for contributors and subagents)

Stack: Rust (Axum, SQLx runtime-checked queries, Postgres, Argon2id, JWT) + Expo React Native (TypeScript strict, Expo Router, TanStack Query, expo-sqlite). Design: `docs/design.md`. Decisions: `docs/decisions.md`.

## Conventions
- JSON over HTTPS. IDs are UUIDs. Times are RFC 3339 UTC. Weight kg, distance m, energy kcal.
- Every request body is validated (`ValidatedJson` + `validator`). Errors: `{"error":{"code","message","fields?"}}`; 400 bad json, 401 auth, 403 forbidden, 404, 409 conflict, 422 validation.
- Every user-owned row has `user_id`; every query filters by the authenticated user. Cross-user access returns 404.
- Syncable tables carry `id` (client-generated UUID), `updated_at`, `deleted_at` (soft delete). Last write wins by `updated_at` (server clamps future timestamps to now()).
- New migrations: `api/migrations/NNNN_name.sql`, forward-only.
- Handlers live in `api/src/<feature>.rs` exposing `routes() -> Router<AppState>`; mounted in `lib.rs`. Tests in `api/tests/<feature>.rs` with `#[sqlx::test]`.
- Feature flags (env `FEATURES=stats,friends,leaderboard,chat`): v2 routes return 404 when off. Mobile reads `GET /config`.

## Data model
- `users(id, email, password_hash, created_at)`; `refresh_tokens` (see auth).
- `profiles(user_id, display_name, calorie_target, weight_goal_kg, units)`.
- `exercises(id, name, muscle_group, source)` — seeded catalog; users may add custom (`owner_id`).
- `workouts(id, user_id, started_at, duration_s, note, source[manual|health], external_id, updated_at, deleted_at)`; `workout_sets(id, workout_id, exercise_id, reps, weight_kg, position)`.
- `foods(id, name, brand, kcal, protein_g, carbs_g, fat_g, serving_g, source[usda|off|custom], external_id, owner_id)`; `food_logs(id, user_id, food_id, logged_at, servings, kcal, protein_g, carbs_g, fat_g, updated_at, deleted_at)`; `favorite_foods(user_id, food_id)`.
- `sleep_logs(id, user_id, bedtime, wake_time, quality 1-5, source, external_id, updated_at, deleted_at)`.
- `weight_logs(id, user_id, measured_at, kg, updated_at, deleted_at)`.
- `health_samples(user_id, category[steps|workouts|sleep], external_id, ...)` — dedupe key `(user_id, category, external_id)`; `health_consents(user_id, category, granted, updated_at)`.
- v2: `character_stats(user_id, strength, cardio, focus, updated_at)`; `friendships(id, requester, addressee, status)`; `invites(code, owner, expires_at, used_by)`; `share_grants(owner, viewer, scope='weekly_summary', granted_at, revoked_at)`; `leaderboard_optin(user_id, opted_in)`; `messages(id, from_user, to_user, body, sent_at)`.

## Endpoints (v1)
Auth: `POST /auth/register|login|refresh|logout`, `GET /me`.
Profile: `GET/PUT /profile`.
Workouts: `GET /workouts?from&to`, `POST /workouts`, `PUT/DELETE /workouts/{id}`; `GET /exercises?q=`.
Food: `GET /foods/search?q=` (local + USDA + OFF), `POST /foods`, `GET/POST/DELETE /food-logs`, `GET/PUT/DELETE /favorites`.
Sleep: `GET/POST /sleep`, `PUT/DELETE /sleep/{id}`. Weight: `GET/POST /weight`.
Dashboard: `GET /dashboard` (calories remaining, last night's sleep + status word, readiness, weight trend).
Health: `PUT /health/consents`, `POST /health/import` (batch, idempotent).
Sync: `POST /sync` `{since, changes:{table:[rows]}}` -> `{server_time, changes}`.
Summary/export: `GET /summary/weekly`, `GET /export?format=json|csv`.

## Status words
See `docs/design.md`. Readiness: `docs/readiness.md`.

## Security
TLS terminates at a reverse proxy in front of the API (see README); Postgres volume and backups must be on encrypted disk, backups additionally encrypted with `age`/gpg (see `scripts/backup.sh`). Secrets only via env. Argon2id passwords; refresh tokens opaque, hashed, rotated.

## Contracts added for parallel work

### GET /dashboard
```json
{
  "name": "Alex",
  "status": { "key": "well_rested|okay|tired|no_data", "label": "Well rested" },
  "readiness": { "score": 82, "components": { "sleep": 90, "training": 100, "recovery": 70 } },
  "sleep": { "minutes": 460, "quality": 4, "label": "Well rested" },
  "diet": { "kcal_eaten": 1420, "kcal_target": 2200, "label": "Under-eating|On track|Overeating" },
  "exercise": { "steps": 6200, "workout_minutes_today": 30, "workout_count_today": 1 },
  "weight": { "kg": 78.4, "trend": "up|flat|down" },
  "stats": { "strength": {"value": 40, "level": 2, "progress": 0.6}, "cardio": {...}, "focus": {...} }
}
```
`stats` is `null` unless the `stats` feature flag is on. Any section with no data returns its numbers as `null` (never errors).

### Health import
`PUT /health/consents` body `{ "steps": bool, "workouts": bool, "sleep": bool }`; `GET /health/consents`.
`POST /health/import` body `{ "steps":[{"external_id","date","count"}], "workouts":[{"external_id","started_at","duration_s","kind"}], "sleep":[{"external_id","bedtime","wake_time"}] }`: idempotent (dedupe by user+source+external_id), ignores categories the user has not consented to (returns per-category counts imported/skipped). Workouts go to `workouts` (source `health`), sleep to `sleep_logs` (source `health`), steps to `health_samples`.

### Friends (v2, flag `friends`)
`POST /friends/invites` -> `{code, link, expires_at}`; `POST /friends/invites/{code}/accept`; `GET /friends`; `DELETE /friends/{user_id}`; `PUT /friends/{user_id}/share` `{enabled}` (weekly summary consent; default off; revoke any time); `GET /friends/{user_id}/summary` returns 403 unless that friend has an active grant to the caller.

### POST /sync (offline sync, last write wins)
Request `{ since: ISO|null, changes: { workouts[], sleep[], weight[], food_logs[] } }` (max 500 rows/table). Each change has `id` (client UUID), `updated_at` (client clock, clamped to server time), `deleted` (soft delete) and the row's fields (workouts carry `sets[]`). Applied only if strictly newer than the stored row; ties keep the server; foreign ids are ignored; invalid rows are listed in `rejected` without blocking others.
Response `{ server_time, has_more, applied:{table:{applied,ignored}}, rejected:[{table,id,reason}], changes:{...rows with deleted flag + synced_at} }`.
Client protocol: keep a local SQLite outbox of unsent changes; on reconnect POST them with `since = last_server_time - 5s`; apply returned rows (newer `updated_at` wins locally too); if `has_more`, repeat with `since = server_time`; store `server_time`.
