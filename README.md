# FitTrack

A simple, open-source personal fitness app: sleep, diet, exercise and weight on one calm home screen.

> Status: under construction. v1 (tracking, readiness, offline sync, export) and v2 (stats, friends, leaderboard, chat) are built behind feature flags.

## Layout

| Path | What |
|---|---|
| `api/` | Rust backend (Axum, SQLx, PostgreSQL) |
| `mobile/` | React Native app (Expo, TypeScript) |
| `docs/` | spec, API, readiness score, QA checklist, decisions, design |

## Run the backend
_Filled in when the API lands._ Outline: `cp .env.example .env`, edit secrets, `docker compose up`.

## Run the app
_Filled in when the app lands._ Outline: `cd mobile && npm install && npx expo start`.

## Install on your phone
- **iPhone:** EAS build + TestFlight (requires an Apple Developer account).
- **Android:** EAS build producing an APK, then sideload.

Details will be added with the first mobile release.

## License
MIT. See [LICENSE](LICENSE). Third-party data/library licenses are logged in [docs/decisions.md](docs/decisions.md).
