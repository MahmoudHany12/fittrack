# QA checklist

Legend: ✅ verified automatically in CI (unit/integration tests) · ☐ needs a human on a device/simulator (iPhone + Android). **Nothing in the ☐ column has been run on a device or simulator yet.**

## Automated (CI)
- ✅ Auth: register, login, refresh rotation + reuse detection, logout, validation, bad token (`api/tests/auth.rs`)
- ✅ Workouts / food / sleep / weight / profile CRUD, validation, cross-user isolation, idempotent client ids
- ✅ Readiness maths and weights (`api/src/readiness.rs`), dashboard aggregation, status words
- ✅ Health import: consent default off, consent respected, idempotent, dedupe, caps, isolation
- ✅ Sync: last-write-wins, ties, deletes/resurrection, foreign rows ignored, per-row rejection, cursor delivery of late edits, clock-skew clamp, size cap
- ✅ Summary + export: math, own-data-only, CSV escaping and injection protection
- ✅ Stats: monotonicity, sleep penalty, decay, levels; flag off → 404
- ✅ Friends permissions: invites, accept rules, share off by default → 403, grant → 200, revoke → 403 immediately, unfriend revokes, non-friend 404, flag off → 404
- ✅ Leaderboard opt-in rules, Chat friend-only access (if merged; see docs/decisions.md)
- ✅ Backup → restore round trip (`scripts/test-restore.sh`)
- ✅ Mobile: validation, API client refresh-on-401, sync engine (outbox, LWW, paging, rejection), home helpers

## Manual: iPhone (simulator minimum) and Android (emulator minimum)
Do each on both platforms; tick when done.

### Account
- ☐ Register with a new email; weak password shows a plain-language error
- ☐ Sign out, sign in again; force-quit and reopen stays signed in
- ☐ Wrong password shows a generic error

### Logging (each under 10 seconds)
- ☐ Log a workout (exercise, sets, reps, weight, duration)
- ☐ Search a food, add it, star a favorite, add the favorite again
- ☐ Log sleep (bedtime, wake, quality stars)
- ☐ Log weight

### Home
- ☐ Four areas show plain status words; each card taps to its log screen
- ☐ Diet bar moves after logging food; weight arrow shows after two weeks of data
- ☐ Strength / Cardio / Focus rings appear with the `stats` flag on, hidden when off
- ☐ Light and dark mode both readable

### Health import (needs a development build)
- ☐ Permission screen has one toggle per category; nothing imports until toggled on
- ☐ iOS: HealthKit permission sheet appears; steps/workouts/sleep import after "Sync now"
- ☐ Android: Health Connect permission sheet appears; data imports
- ☐ Turning a category off stops future imports; running Sync twice creates no duplicates

### Offline
- ☐ Airplane mode: log workout, food, sleep, weight; app stays usable
- ☐ Reconnect: items appear on the server (check via a second device or `GET /export`)
- ☐ Edit the same entry on two devices offline: the later edit wins after both sync

### Summary & export
- ☐ Weekly summary reads in plain words; empty week shows the empty state
- ☐ Export JSON and CSV open the share sheet; files contain only your data

### Friends (v2)
- ☐ Invite link/code works; accepting adds both users as friends
- ☐ Sharing is off by default; friend sees "hasn't shared" until you switch it on; switching off hides it at once
- ☐ Unfriend removes access both ways
- ☐ Leaderboard shows only opted-in friends; opting out removes you
- ☐ Chat messages arrive between friends; non-friends cannot message

### Stability
- ☐ No crash through all the above; rotate device, background/foreground the app during a sync
