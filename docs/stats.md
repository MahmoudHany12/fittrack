# Game-style stats (v2, feature flag `stats`)

> **Levels and values now come from the progression model: see
> [progression.md](progression.md).** The 28-day formula, inactivity decay, sleep
> penalty and body-composition boost that used to live here are gone.

Three stats, each `value` 0-100: **Strength**, **Cardio**, **Focus**.
`level` is 1-4 and `progress` is 0..1 within the level; the UI shows "Level n/4".
Per progression.md, points (+1 per on-time session, -1 per missed window; Focus
+1 per distinct active day) map to levels at 0 / 4 / 12 / 24 points, and
`value = (level - 1) * 25 + 25 * progress`.

## Endpoint

`GET /stats` (auth required, 404 when the `stats` flag is off):

```json
{ "strength": {"value": 40, "level": 2, "progress": 0.6}, "cardio": {...}, "focus": {...} }
```

The body is `null` (HTTP 200) when the user has no (non-deleted) workouts in the
last 365 days. Other modules call `stats::current_stats(pool, user_id, now) ->
Result<Option<Stats>, sqlx::Error>`, which also upserts the daily snapshot in
`character_stats(user_id, day, strength, cardio, focus, computed_at)` (one
upsert per call; every query filters by user id), and
`stats::current_stats_or_zero`, which returns `Stats::zero()` (value 0, level 1,
progress 0) instead of `None`.

`GET /progression`, `GET /records` and `GET /program` are described in
progression.md and share the same `stats` flag.

## Dashboard

On `GET /dashboard` with the `stats` flag on, `stats` is **always an object**
(zeroed for users with no activity; the zero object is not stored in
`character_stats`) and `progression` carries the full progression object.
`workout_window.fraction` follows the cardio track's window fraction.
