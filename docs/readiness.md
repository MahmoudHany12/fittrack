# Readiness score (0–100)

`readiness = 0.50·sleep + 0.30·training + 0.20·recovery`, each component 0–100. Weights live in `api/src/readiness.rs` (`Weights::default()`, overridable via env `READINESS_W_SLEEP/TRAINING/RECOVERY`, renormalised to sum 1).

## Components
- **Sleep (last night):** hours score = 100 at 7.5–9 h, linearly down to 0 at 3 h and to 60 at 11 h; blended `0.8·hours + 0.2·(quality-1)/4·100` when a quality rating exists. No data → component is *missing*.
- **Training load (last 7 days):** training "freshness". Load = Σ minutes of workouts in the last 3 days weighted 1.0 plus days 4–7 weighted 0.4. Score = 100 for load ≤ 90 (comfortable), declining linearly to 20 at load ≥ 400. No workouts at all → 85 (rested, slightly detrained).
- **Recovery:** days since last workout ≥ 2 → 100; same day → 40; yesterday → 70. Reduced by 15 if the previous night's sleep < 6 h.

## Missing data
Missing components are dropped and the remaining weights are renormalised. With no data at all readiness is `null` and the UI shows "No data yet".

## Status words
Readiness ≥ 70 "Well rested", 40–69 "Okay", < 40 "Tired".
