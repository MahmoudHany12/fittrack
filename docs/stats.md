# Game-style stats (v2, feature flag `stats`)

Three stats, each `value` 0-100: **Strength**, **Cardio**, **Focus**.
`level` is 1-4 (0-24 -> 1, 25-49 -> 2, 50-74 -> 3, 75-100 -> 4) and
`progress` is 0..1 within the level (`(value - level_floor) / 25`, 1.0 at 100).
The UI shows "Level n/4".

## Endpoint

`GET /stats` (auth required, 404 when the `stats` flag is off):

```json
{ "strength": {"value": 40, "level": 2, "progress": 0.6}, "cardio": {...}, "focus": {...} }
```

The body is `null` (HTTP 200) when the user has no workouts or sleep in the
last 28 days. Other modules call `stats::current_stats(pool, user_id, now) ->
Result<Option<Stats>, sqlx::Error>`, which also upserts the daily snapshot in
`character_stats(user_id, day, strength, cardio, focus, computed_at)` (one
upsert per call; every query filters by user id).

## Formulas (`stats::compute_stats(inputs, now)`, pure, deterministic)

Baseline is 10. `sat(points, scale) = 10 + 90 * (1 - exp(-points / scale))`
(diminishing returns).

- **Strength** (28 days): `points = 6 * sessions + sets` over workouts with
  at least one non-cardio set; `raw = sat(points, 60)`.
- **Cardio** (28 days): `minutes` of cardio-group work; `raw = sat(minutes, 300)`.
  A workout's duration is split by set count between cardio and other groups;
  a workout with no sets (e.g. a health import) counts fully as cardio.
- **Focus** (14 days): `raw = 10 + 90 * (0.5 * min(good_nights / 10, 1) +
  0.5 * min(active_days / 6, 1))`, where a good night is >= 7 h slept (naps on
  the same wake date are summed) and active days are distinct workout dates.

Then, for each stat:

1. **Inactivity decay**: `value = 10 + (raw - 10) * keep`, with
   `keep = 1 - days_since_last_workout / 14` clamped to 0..1 (no workout in
   14 days, or ever, gives the baseline).
2. **Sleep penalty**: if the average of sleep logs woken in the last 7 days is
   under 6 h, multiply by `1 - 0.2 * min((6 - avg) / 2, 1)` (up to 20% at 4 h).
   No sleep data means no penalty.
3. Round and clamp to 0..100.

Workouts or sleep in the future and anything older than the window are ignored.

## Body composition boost and the dashboard

When the user has a body scan with skeletal muscle mass, **Strength** gets a
small boost added after steps 1-3: with `r = skeletal_muscle_kg / weight_kg`,
`boost = 10 * clamp((r - lo) / (hi - lo), 0, 1)` where `(lo, hi)` is
`(0.35, 0.45)` for male, `(0.28, 0.38)` for female and `(0.315, 0.415)` when
the profile has no sex. The result is clamped to 100. Cardio and Focus are
unchanged.

On `GET /dashboard` with the `stats` flag on, `stats` is **always an object**:
users with no workouts or sleep get `Stats::zero()` (value 0, level 1,
progress 0 per stat; `stats::current_stats_or_zero`). `GET /stats` keeps
returning `null` for them, and the zero object is not stored in
`character_stats`.
