# FitTrack

A simple, open-source personal fitness app: sleep, diet, exercise and weight on one calm home screen.

- **v1:** accounts, workout / food / sleep / weight logging, automatic health import (HealthKit + Health Connect), dashboard with a readiness status, weekly summary, offline logging with sync, JSON/CSV export.
- **v2 (feature flags):** game-style stats (Strength / Cardio / Focus), friends with explicit, revocable summary sharing, opt-in leaderboard, chat.

| Path | What |
|---|---|
| `api/` | Rust backend (Axum, SQLx, PostgreSQL, Argon2id, JWT) |
| `mobile/` | React Native app (Expo, TypeScript strict, Expo Router, TanStack Query, expo-sqlite) |
| `docs/` | `spec.md`, `api.md`, `readiness.md`, `stats.md`, `qa.md`, `design.md`, `decisions.md` |
| `scripts/` | backup / restore / test-restore, local checks |

## Run the backend

Requires Docker.

```bash
cp .env.example .env        # then edit every CHANGE_ME (use `openssl rand -base64 48` for secrets)
docker compose up --build   # Postgres + API on http://localhost:8080 ; GET /health -> ok
```

`FEATURES=stats,friends,leaderboard,chat` in `.env` turns on the v2 features (leave empty for v1 only). `USDA_API_KEY` (free, https://fdc.nal.usda.gov/api-key-signup) enables USDA food search; Open Food Facts works without a key.

Without Docker: install Rust + Postgres 16, set `DATABASE_URL`, `JWT_ACCESS_SECRET`, then `cd api && cargo run`. Migrations run on start.

**TLS:** the API speaks plain HTTP and is meant to sit behind a TLS-terminating reverse proxy (Caddy, nginx, Cloudflare Tunnel). Never expose port 8080 to the internet directly; the app must point at an `https://` URL in production. Keep the Postgres volume on an encrypted disk.

### Backups
`docker compose` runs a `backup` service that dumps the database every 24 h into `./backups` (set `AGE_RECIPIENT` to an [age](https://age-encryption.org) public key to encrypt dumps; keep the private key offline). Restore with `scripts/restore.sh <dump> <db>`; `scripts/test-restore.sh` proves the round trip and runs in CI.

## Run the app

```bash
cd mobile
npm install
cp .env.example .env        # EXPO_PUBLIC_API_URL=http://<your computer's LAN IP>:8080
npx expo start
```

Expo Go is enough for everything except health import. HealthKit / Health Connect need a **development build** (native modules): `npx expo run:android` (Android Studio + emulator) or `npx expo run:ios` (macOS + Xcode), or an EAS development build (below). On an emulator use `http://10.0.2.2:8080` for the API.

## Install on your phone

### Fastest (today, free): Expo Go
Works for everything except automatic Apple Health / Health Connect import.
1. Start the backend (above) on your PC and find its Wi-Fi IP (`ipconfig` → Wi-Fi IPv4, e.g. `192.168.1.22`).
2. `cd mobile && npx expo start --lan` (leave `EXPO_PUBLIC_API_URL` empty: in development the app talks to the same computer on port 8080, so a changed Wi-Fi IP needs no config).
3. On the iPhone: install **Expo Go** from the App Store, join the same Wi-Fi, scan the QR code with the Camera app (or type `exp://<IP>:8081` in Expo Go). Allow Node.js/Docker through the Windows firewall on private networks if asked.

The PC must stay on for the phone to sync; entries logged while it is off are kept on the phone and sync later.

### Permanent install (health import included)

Builds use [EAS](https://docs.expo.dev/build/introduction/) (free tier is enough). One-time: `npm i -g eas-cli && eas login`, then set your real API URL in `mobile/eas.json` (`YOUR-API-HOST`).

- **Android (APK):** `cd mobile && eas build -p android --profile preview` → download the `.apk` from the link EAS prints, open it on the phone (allow "install unknown apps").
- **iPhone (TestFlight):** needs a paid Apple Developer account. `eas build -p ios --profile production` then `eas submit -p ios`; invite yourself in App Store Connect → TestFlight.

Bundle id / package: `dev.mahmoudhany12.fittrack` (change in `mobile/app.json` if you publish under your own).

## Develop

See [CONTRIBUTING.md](CONTRIBUTING.md). Checks: API `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (integration tests need `DATABASE_URL`); mobile `npm run lint && npm run typecheck && npm test`. CI runs all of them plus the backup-restore test.

## License
MIT. See [LICENSE](LICENSE). Third-party data and library licenses are logged in [docs/decisions.md](docs/decisions.md).
