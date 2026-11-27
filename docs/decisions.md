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
