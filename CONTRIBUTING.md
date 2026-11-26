# Contributing

## Branches
- `main`: protected; PR + passing CI required.
- `develop`: integration branch. Open PRs here.
- `feature/<name>`: one feature per branch.
- `release/v1`, `release/v2`: cut from `develop` when ready.

## Workflow
1. Branch from `develop`: `git switch -c feature/<name> develop`.
2. Small commits with clear messages.
3. Open a PR into `develop`; CI must pass.
4. Update `docs/spec.md` and log decisions in `docs/decisions.md`.

## Checks
- API: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- Mobile: `npm run lint`, `npm run typecheck`, `npm test`.

## Rules
- No secrets in the repo; use `.env.example`.
- Check the license of any library or code before reuse (see `docs/decisions.md`). Do not copy AGPL code (e.g. wger) into this repo.
- Use original names, art and colors only.
