# API reference

Base URL: `http://localhost:8080` (put TLS in front in production). JSON everywhere. Authenticated routes need `Authorization: Bearer <access_token>`. Errors: `{"error":{"code","message","fields?"}}` with 400 bad JSON, 401, 403, 404, 409, 422 validation.

The detailed contracts and conventions live in [spec.md](spec.md); this page is the route index.

| Area | Routes |
|---|---|
| Health check | `GET /health` |
| Auth | `POST /auth/register`, `/auth/login`, `/auth/refresh`, `/auth/logout`; `GET /me` |
| Profile | `GET/PUT /profile` |
| Workouts | `GET /exercises?q=`, `POST /exercises`; `GET/POST /workouts`, `PUT/DELETE /workouts/{id}` |
| Food | `GET /foods/search?q=`, `POST /foods`; `GET/POST /food-logs`, `PUT/DELETE /food-logs/{id}`, `GET /food-logs/summary?date=`; `GET /favorites`, `PUT/DELETE /favorites/{food_id}` |
| Sleep | `GET/POST /sleep`, `PUT/DELETE /sleep/{id}` |
| Weight | `GET/POST /weight`, `PUT/DELETE /weight/{id}` |
| Dashboard | `GET /dashboard` |
| Health import | `GET/PUT /health/consents`, `POST /health/import` |
| Sync | `POST /sync` |
| Summary / export | `GET /summary/weekly`, `GET /export?format=json|csv&table=` |
| Stats (flag `stats`) | `GET /stats` |
| Friends (flag `friends`) | `POST /friends/invites`, `POST /friends/invites/{code}/accept`, `GET /friends`, `DELETE /friends/{id}`, `PUT /friends/{id}/share`, `GET /friends/{id}/summary` |
| Leaderboard (flag `leaderboard`) | `GET/PUT /leaderboard/optin`, `GET /leaderboard` |
| Chat (flag `chat`) | `GET /chat`, `GET/POST /chat/{friend_id}`, `DELETE /chat/{friend_id}/messages/{id}` |

## Auth tokens
`access_token` is a 15-minute JWT. `refresh_token` is opaque, valid 30 days, **rotated on every use**; replaying an already-used refresh token revokes all of that user's sessions.

## Feature flags
`FEATURES=stats,friends,leaderboard,chat` (comma list, env). A disabled feature's routes return 404.
