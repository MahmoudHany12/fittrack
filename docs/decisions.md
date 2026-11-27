# Decisions & reused projects

## Decisions
| Date | Decision | Why |
|---|---|---|
| 2026-11-27 | Monorepo: `api/`, `mobile/`, `docs/` | One PR can change API + app together |
| 2026-11-27 | Repo is public, MIT licensed | Chosen by owner |
| 2026-11-27 | CI jobs skip until `api/Cargo.toml` / `mobile/package.json` exist; a single `ci-ok` job is the required check | Lets branch protection be on from day one |

## Reused projects
| Project | Use | License | Notes |
|---|---|---|---|
| wger | Reference + exercise data/API only | AGPL-3.0 | No code copied |
| SparkyFitness | Data model reference | (check before reuse) | Study only until license verified |
| Open Food Facts | Food search data | ODbL (data) | Attribution required |
| USDA FoodData Central | Food search data | CC0 / public domain | API key required |

## Auth (feature/auth)
| Date | Decision | Why |
|---|---|---|
| 2026-11-27 | Refresh tokens are opaque 256-bit values stored as SHA-256, rotated per use, with reuse detection | Revocable server-side; a leaked JWT refresh token could not be revoked. Access tokens are HS256 JWTs (15 min) |
| 2026-11-27 | SQLx runtime-checked queries (`query_as`) instead of compile-time `query!` | Offline query data can't be generated without a working local toolchain; every query is exercised by `#[sqlx::test]` against real Postgres in CI. Revisit later |
| 2026-11-27 | TLS terminates at a reverse proxy; at-rest encryption via encrypted disk + encrypted backups | Standard deployment; keeps the API simple |
| 2026-11-27 | Dark-first design inspired by the feel of a phone health widget; own palette/icons/copy | Owner feedback; no third-party game assets |
| 2026-11-27 | Rust is verified by GitHub CI (not locally) when Docker Desktop can't start | C: drive was full; do not delete user data |
| 2026-11-27 | Mobile deps: Expo SDK 58, TanStack Query, expo-secure-store, lucide-react-native (ISC), react-native-svg (MIT) | Maintained libraries |

## Health import (feature/health)
| Date | Decision | Why |
|---|---|---|
| 2026-11-27 | iOS reads HealthKit via `@kingstinct/react-native-healthkit` (MIT, Nitro module); Android reads Health Connect via `react-native-health-connect` (MIT). Not Google Fit (deprecated) | Maintained libraries; read-only, per-category permissions |
| 2026-11-27 | Both need native code: a development build (EAS or `expo run:*`), not Expo Go. Config plugins added to `mobile/app.json` (plus `expo-build-properties` for Android minSdk 26 and `react-native-nitro-modules`) | Expo Go cannot load them. Native modules are lazy-required behind `src/health/provider.ts`, so Expo Go, web and jest get a no-op provider |
| 2026-11-27 | Consent is per category (steps, workouts, sleep), default off, enforced server-side; revoking stops future imports but keeps imported data | Health data is sensitive; the user deletes data through the normal screens. Payloads are never logged |
| 2026-11-27 | Steps are stored once per local day (`steps-YYYY-MM-DD`), updated if the count grows; workouts/sleep dedupe by (user, source, external_id) | Idempotent re-sync; no minute-level data |
