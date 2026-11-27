# Body composition, energy targets and the Home cards

How leading apps do it, and what FitTrack copies:
- **MacroFactor / MyFitnessPal / Cronometer:** a calorie target comes from an estimate of energy expenditure (BMR × activity) adjusted for the goal; MacroFactor then *adapts* the expenditure estimate from logged intake vs. the weight trend.
- **InBody / smart scales:** body fat %, skeletal muscle mass and a measured BMR are better inputs than weight alone. Lean mass drives BMR (Katch-McArdle) and protein needs.
- **Whoop / Oura / Apple Fitness:** a short daily activity goal ("ring"), a readiness word, few numbers.

## Profile fields (all optional, validated)
`height_cm` 100..250 · `birth_date` (age 13..100) · `sex` `male|female` · `activity_level` `sedentary|light|moderate|active|very_active` · `goal` `lose|maintain|gain` · `goal_rate_kg_per_week` 0..1 (default 0.5 lose / 0.25 gain) · `calorie_target_mode` `auto|manual` (default `auto`; `manual` keeps `calorie_target`).

## Body composition (InBody or manual)
`body_compositions(id client-uuid, user_id, measured_at, weight_kg 20..400, body_fat_pct 2..75 null, skeletal_muscle_kg 5..100 null, fat_mass_kg null, visceral_fat_level 1..30 null, bmr_kcal 600..5000 null, body_water_pct 20..80 null, source inbody|manual, updated_at, deleted_at)`.
Saving one also upserts a `weight_logs` row with the same id/measured_at/kg, so the weight trend includes scans.
API: `GET /body-composition?from&to` (newest first), `POST /body-composition` (idempotent by client id, owner-only), `PUT/DELETE /body-composition/{id}`.

## Energy and macro targets (`GET /targets`)
Inputs: latest weight (weight_logs), latest body composition (≤ 90 days old), profile.

1. **BMR** (first that applies):
   - `inbody`: measured `bmr_kcal` from the latest scan.
   - `katch_mcardle`: 370 + 21.6 × LBM, LBM = weight × (1 − body_fat_pct/100).
   - `mifflin`: 10·kg + 6.25·cm − 5·age + 5 (male) / −161 (female). Needs height, age, sex.
   - otherwise `null` (list the missing fields).
2. **TDEE (formula)** = BMR × {sedentary 1.2, light 1.375, moderate 1.55, active 1.725, very_active 1.9} (default moderate).
3. **TDEE (adaptive)**, MacroFactor-style: with ≥ 14 of the last 21 days having food logs and ≥ 2 weights at least 14 days apart in that window: `avg_daily_intake − (Δweight_kg × 7700) / days`. Final TDEE = 0.5·formula + 0.5·adaptive when both exist, clamped to ±25% of formula; `tdee_source` = `formula|adaptive|blended`.
4. **Calorie target** = TDEE − goal_rate × 7700 / 7 (lose) / + (gain) / 0 (maintain), never below max(BMR, 1200 female / 1500 male / 1200 unknown); rounded to 10 kcal. With `calorie_target_mode=manual`, use the profile value.
5. **Macros:** protein = 2.0 g/kg lean mass when body fat is known, else 1.6 g/kg body weight; fat = max(0.6 g/kg, 27% of kcal / 9); carbs = remaining kcal / 4 (min 0). All rounded to whole grams.

Response: `{ bmr, bmr_source, tdee, tdee_source, calorie_target, calorie_target_source: "auto"|"manual"|"default", protein_g, fat_g, carbs_g, lean_mass_kg, body_fat_pct, missing: ["height_cm", ...] }` — numbers nullable; when nothing is computable `calorie_target` falls back to the profile value (default 2200, source `default`).

## Dashboard additions
- `diet.kcal_target` = `/targets` calorie_target; add `diet.protein_g_eaten` and `diet.protein_target_g`.
- `body`: `{ weight_kg, trend, body_fat_pct, skeletal_muscle_kg, measured_at }` (latest scan; nulls if none).
- `workout_window`: `{ minutes_today, goal_minutes: 30, fraction: 0..1 }` — today's workout minutes (+ steps/100 as active-minute proxy when steps exist), WHO 150 min/week ≈ 30/day.
- `stats` is **always an object** when the `stats` flag is on (level 1, progress 0, value 0 when there is no activity yet), so the Home cards always render.
- Strength stat also gets a small boost from skeletal-muscle-mass relative to body weight when a scan exists (SMM/weight ≥ 0.45 male / 0.38 female → +10, scaled linearly from 0.35/0.28); documented in docs/stats.md.

## Home layout (matches the owner's reference, own colors/icons)
Header: name, status word under it (readiness ≥70 "Healthy", 40–69 "Okay", <40 "Tired", none "No data yet"), round avatar with initials at the right.
"Fitness": **Strength Workout** card (title in rose, "Level n/4", ring with heart icon), **Cardio Workout** card (title in mint, ring with lightning icon) containing a **Workout window** bar (hourglass icon), **Focus** card (title in lavender, ring with eye icon). Selected/primary card has a light outline.
"Body & Nutrition": Weight (kg + arrow) with body fat % and muscle kg when a scan exists, Diet (kcal eaten / target bar + protein), Sleep (hours + word). Every card opens its log screen; every pushed screen has a back button.
