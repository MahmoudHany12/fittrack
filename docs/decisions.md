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
