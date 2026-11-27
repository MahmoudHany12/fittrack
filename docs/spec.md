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
