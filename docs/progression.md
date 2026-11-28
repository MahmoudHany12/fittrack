# Progression: workout windows, levels, recovery (v2, flag `stats`)

Inspired by the "game fitness app" pattern the owner referenced (levels out of 4, a draining
workout window, rest and protein shortening recovery, Focus that only grows). Own names and art.
Replaces the 28-day formula in docs/stats.md for levels; the 0–100 `value` remains for the rings.

## Concepts
- **Track:** `strength` and `cardio` (trained by workouts), `focus` (trained by focus activities:
  exercises in muscle group `focus` — meditation, yoga, stretching, basketball, golf, swimming
  laps for technique, etc.; seed ~10 such exercises in a migration).
- A workout counts for a track by its sets: any non-cardio, non-focus set → strength; cardio sets
  or no sets (duration only / health import) → cardio; focus sets → focus. One workout can count
  for several tracks.

## Workout window (strength and cardio)
State machine per track, computed from history (no cron):
1. After a counted session at time `t`, the track enters **recovering** until
   `t + recovery`, with `recovery` = 20 h (strength) / 12 h (cardio).
   - **Protein bonus:** ≥ 25 g protein logged within 2 h after the session halves the recovery.
   - **Rest requirement:** the window only opens after at least one night of ≥ 6 h sleep that
     ends after `t` (if the user logs no sleep at all in the last 7 days, this rule is skipped).
2. Then the **window is open** for 48 h. Training inside it counts as an *on-time session*.
3. If the window closes without a session the track **slips**: one missed window.
4. Training while recovering is allowed and logged, but does not count as an on-time session
   ("Your body is still recovering").
- First ever session for a track opens the cycle; before that the window is "open" (start now).

API exposes per track: `state: "open"|"recovering"|"needs_rest"|"missed"`, `opens_at`, `closes_at`,
`fraction` (open: remaining share of the 48 h window, draining 1→0; recovering: progress 0→1 to
opening), and `protein_bonus` bool for the current recovery.

## Levels (1–4) and progress
- Each on-time session gives **+1 point**; each missed window **−1 point** (never below 0).
  Focus only gains (+1 per focus session on a distinct day, no penalties, no windows).
- Level thresholds (points): L1 0, L2 4, L3 12, L4 24. `progress` = share toward the next level
  (1.0 at L4 with ≥ 24 points). Demotion happens if points fall below a threshold.
- `value` (0–100) for the ring = level floor (0/25/50/75) + 25 × progress.

## Daily ring and post-workout
- **Daily ring:** closes when today's active minutes (workouts + steps/100) ≥ 30. Once closed:
  "Done for today — extra training won't add more."
- After saving a workout: "Workout complete — time to rest and recover." plus, if no protein yet
  within 2 h: "Have ~25 g protein in the next 2 hours to recover twice as fast."

## Fuel and body
- **Fuel hint:** if a window is open and no food was logged in the last 4 h: "Eat something
  before training for more energy."
- **Energy balance today:** kcal eaten − TDEE (from /targets). Weekly projection in kg
  (`balance_7d_avg × 7 / 7700`) shown on the Body card: "At this pace: −0.3 kg/week".

## Personal bests
- Per exercise: heaviest weight (any reps) and best estimated 1RM (Epley: w × (1 + reps/30)).
  A new best when a workout is saved → `personal_bests` in the workout response and a
  `GET /records` list; Home shows a small "New personal best" badge for 24 h.

## Starter program (built in)
"Strength basics" — 3 days/week, 4 weeks: back squat 3×5 + kettlebell swing 3×15 (superset),
bench press 3×5, deadlift 3×5, pull-up 3×5. Rest 60–120 s. Weeks 3–4 add a fourth set.
Progression: +2.5 kg squat/bench, +5 kg deadlift after a session where all reps were done;
after two sessions in a row with missed reps, −10 %.
`GET /program` returns today's suggested session with target weights computed from the user's
last sessions (or "pick a starting weight" when none).

## API summary
- `GET /progression` → `{ tracks: { strength: {...window, level, points, progress, value} (level fields flattened into each track),
  cardio: {...}, focus: {level, points, progress, value} }, ring: {minutes, goal: 30, closed},
  hints: [string], new_personal_bests: [...] }`
- Dashboard `stats` comes from this model; dashboard gains `progression` (same object) and
  `workout_window` uses the cardio track's window fraction when available.
- `GET /records`, `GET /program`.
