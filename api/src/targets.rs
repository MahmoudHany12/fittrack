//! Energy and macro targets (see docs/body.md). All maths are pure functions;
//! `compute_for_user` loads the inputs (every query filtered by user id).

use crate::{auth::AuthUser, body, error::AppError, profile, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/targets", get(get_targets))
}

/// kcal per kg of body weight change.
pub const KCAL_PER_KG: f64 = 7700.0;
pub const ADAPTIVE_WINDOW_DAYS: i64 = 21;
pub const ADAPTIVE_MIN_FOOD_DAYS: usize = 14;
pub const ADAPTIVE_MIN_WEIGHT_SPAN_DAYS: f64 = 14.0;
pub const SCAN_MAX_AGE_DAYS: i64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
}

impl Sex {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "male" => Some(Sex::Male),
            "female" => Some(Sex::Female),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub weight_kg: Option<f64>,
    pub height_cm: Option<f64>,
    pub age: Option<i32>,
    pub sex: Option<Sex>,
    pub activity_level: Option<String>,
    pub goal: Option<String>,
    pub goal_rate_kg_per_week: Option<f64>,
    pub manual_mode: bool,
    /// The profile's `calorie_target` (used in manual mode and as the fallback).
    pub profile_target: i32,
    pub scan_bmr_kcal: Option<i32>,
    pub body_fat_pct: Option<f64>,
    /// Output of [`adaptive_tdee`], if enough data exists.
    pub adaptive_tdee: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Targets {
    pub bmr: Option<i32>,
    pub bmr_source: Option<&'static str>,
    pub tdee: Option<i32>,
    pub tdee_source: Option<&'static str>,
    pub calorie_target: i32,
    pub calorie_target_source: &'static str,
    pub protein_g: Option<i32>,
    pub fat_g: Option<i32>,
    pub carbs_g: Option<i32>,
    pub lean_mass_kg: Option<f64>,
    pub body_fat_pct: Option<f64>,
    pub missing: Vec<&'static str>,
}

/// Whole years between `birth` and `today`.
pub fn age_years(birth: NaiveDate, today: NaiveDate) -> i32 {
    use chrono::Datelike;
    let mut age = today.year() - birth.year();
    if (today.month(), today.day()) < (birth.month(), birth.day()) {
        age -= 1;
    }
    age
}

pub fn mifflin_st_jeor(kg: f64, cm: f64, age: i32, sex: Sex) -> f64 {
    let base = 10.0 * kg + 6.25 * cm - 5.0 * f64::from(age);
    match sex {
        Sex::Male => base + 5.0,
        Sex::Female => base - 161.0,
    }
}

pub fn lean_mass(kg: f64, body_fat_pct: f64) -> f64 {
    kg * (1.0 - body_fat_pct / 100.0)
}

pub fn katch_mcardle(lean_mass_kg: f64) -> f64 {
    370.0 + 21.6 * lean_mass_kg
}

pub fn activity_multiplier(level: Option<&str>) -> f64 {
    match level {
        Some("sedentary") => 1.2,
        Some("light") => 1.375,
        Some("active") => 1.725,
        Some("very_active") => 1.9,
        _ => 1.55, // moderate and the default
    }
}

/// First applicable BMR: InBody measured, Katch-McArdle, Mifflin-St Jeor.
pub fn compute_bmr(i: &Inputs) -> Option<(f64, &'static str)> {
    if let Some(b) = i.scan_bmr_kcal {
        return Some((f64::from(b), "inbody"));
    }
    if let (Some(kg), Some(bf)) = (i.weight_kg, i.body_fat_pct) {
        return Some((katch_mcardle(lean_mass(kg, bf)), "katch_mcardle"));
    }
    if let (Some(kg), Some(cm), Some(age), Some(sex)) = (i.weight_kg, i.height_cm, i.age, i.sex) {
        return Some((mifflin_st_jeor(kg, cm, age, sex), "mifflin"));
    }
    None
}

/// MacroFactor-style expenditure estimate from the last 21 days.
/// `weights` are the (time, kg) samples inside the window. Needs at least 14
/// days with food logs and two weights at least 14 days apart.
pub fn adaptive_tdee(
    days_with_food: usize,
    total_kcal: f64,
    weights: &[(DateTime<Utc>, f64)],
) -> Option<f64> {
    if days_with_food < ADAPTIVE_MIN_FOOD_DAYS {
        return None;
    }
    let first = weights.iter().min_by_key(|w| w.0)?;
    let last = weights.iter().max_by_key(|w| w.0)?;
    let span_days = (last.0 - first.0).num_seconds() as f64 / 86_400.0;
    if span_days < ADAPTIVE_MIN_WEIGHT_SPAN_DAYS {
        return None;
    }
    let avg_intake = total_kcal / days_with_food as f64;
    let delta = last.1 - first.1;
    Some(avg_intake - delta * KCAL_PER_KG / span_days)
}

/// Final TDEE and its source. Blends 50/50 and clamps to +-25% of the formula.
pub fn blend_tdee(formula: Option<f64>, adaptive: Option<f64>) -> Option<(f64, &'static str)> {
    match (formula, adaptive) {
        (Some(f), Some(a)) => {
            let blended = (0.5 * f + 0.5 * a).clamp(f * 0.75, f * 1.25);
            Some((blended, "blended"))
        }
        (Some(f), None) => Some((f, "formula")),
        (None, Some(a)) => Some((a.max(0.0), "adaptive")),
        (None, None) => None,
    }
}

fn round10(x: f64) -> i32 {
    ((x / 10.0).round() * 10.0) as i32
}

fn ceil10(x: f64) -> i32 {
    ((x / 10.0).ceil() * 10.0) as i32
}

/// Goal-adjusted calorie target, floored and rounded to 10 kcal.
pub fn calorie_target(
    tdee: f64,
    goal: Option<&str>,
    rate_kg_per_week: Option<f64>,
    bmr: Option<f64>,
    sex: Option<Sex>,
) -> i32 {
    let daily = |rate: f64| rate * KCAL_PER_KG / 7.0;
    let raw = match goal {
        Some("lose") => tdee - daily(rate_kg_per_week.unwrap_or(0.5)),
        Some("gain") => tdee + daily(rate_kg_per_week.unwrap_or(0.25)),
        _ => tdee,
    };
    let sex_floor = match sex {
        Some(Sex::Male) => 1500.0,
        _ => 1200.0,
    };
    let floor = bmr.unwrap_or(0.0).max(sex_floor);
    round10(raw).max(ceil10(floor))
}

/// (protein, fat, carbs) in whole grams.
pub fn macros(kcal: i32, weight_kg: f64, lean_mass_kg: Option<f64>) -> (i32, i32, i32) {
    let kcal = f64::from(kcal);
    let protein = match lean_mass_kg {
        Some(l) => 2.0 * l,
        None => 1.6 * weight_kg,
    };
    let fat = (0.6 * weight_kg).max(0.27 * kcal / 9.0);
    let carbs = ((kcal - 4.0 * protein - 9.0 * fat) / 4.0).max(0.0);
    (
        protein.round() as i32,
        fat.round() as i32,
        carbs.round() as i32,
    )
}

pub fn compute(i: &Inputs) -> Targets {
    let bmr = compute_bmr(i);
    let formula_tdee = bmr.map(|(b, _)| b * activity_multiplier(i.activity_level.as_deref()));
    let tdee = blend_tdee(formula_tdee, i.adaptive_tdee);

    let mut missing = Vec::new();
    if bmr.is_none() {
        if i.weight_kg.is_none() {
            missing.push("weight_kg");
        }
        if i.height_cm.is_none() {
            missing.push("height_cm");
        }
        if i.age.is_none() {
            missing.push("birth_date");
        }
        if i.sex.is_none() {
            missing.push("sex");
        }
    }

    let (target, source) = if i.manual_mode {
        (i.profile_target, "manual")
    } else if let Some((t, _)) = tdee {
        (
            calorie_target(
                t,
                i.goal.as_deref(),
                i.goal_rate_kg_per_week,
                bmr.map(|b| b.0),
                i.sex,
            ),
            "auto",
        )
    } else {
        (i.profile_target, "default")
    };

    let lean = match (i.weight_kg, i.body_fat_pct) {
        (Some(kg), Some(bf)) => Some(lean_mass(kg, bf)),
        _ => None,
    };
    let (protein_g, fat_g, carbs_g) = match i.weight_kg {
        Some(kg) => {
            let (p, f, c) = macros(target, kg, lean);
            (Some(p), Some(f), Some(c))
        }
        None => (None, None, None),
    };

    Targets {
        bmr: bmr.map(|b| b.0.round() as i32),
        bmr_source: bmr.map(|b| b.1),
        tdee: tdee.map(|t| t.0.round() as i32),
        tdee_source: tdee.map(|t| t.1),
        calorie_target: target,
        calorie_target_source: source,
        protein_g,
        fat_g,
        carbs_g,
        lean_mass_kg: lean.map(|l| (l * 10.0).round() / 10.0),
        body_fat_pct: i.body_fat_pct,
        missing,
    }
}

pub async fn compute_for_user(
    pool: &PgPool,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Targets, sqlx::Error> {
    let prof = profile::load(pool, user_id).await?;
    let scan = body::latest(pool, user_id, now)
        .await?
        .filter(|s| now - s.measured_at <= Duration::days(SCAN_MAX_AGE_DAYS));

    let latest_weight: Option<f64> = sqlx::query_scalar(
        "SELECT kg::float8 FROM weight_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND measured_at <= $2 \
         ORDER BY measured_at DESC, created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    let since = now - Duration::days(ADAPTIVE_WINDOW_DAYS);
    let (food_days, total_kcal): (i64, Option<f64>) = sqlx::query_as(
        "SELECT COUNT(DISTINCT (logged_at AT TIME ZONE 'UTC')::date), SUM(kcal)::float8 \
         FROM food_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND logged_at > $2 AND logged_at <= $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_one(pool)
    .await?;
    let weights: Vec<(DateTime<Utc>, f64)> = sqlx::query_as(
        "SELECT measured_at, kg::float8 FROM weight_logs \
         WHERE user_id = $1 AND deleted_at IS NULL AND measured_at > $2 AND measured_at <= $3",
    )
    .bind(user_id)
    .bind(since)
    .bind(now)
    .fetch_all(pool)
    .await?;

    let inputs = Inputs {
        weight_kg: latest_weight.or(scan.as_ref().map(|s| s.weight_kg)),
        height_cm: prof.height_cm,
        age: prof.birth_date.map(|b| age_years(b, now.date_naive())),
        sex: prof.sex.as_deref().and_then(Sex::parse),
        activity_level: prof.activity_level,
        goal: prof.goal,
        goal_rate_kg_per_week: prof.goal_rate_kg_per_week,
        manual_mode: prof.calorie_target_mode == "manual",
        profile_target: prof.calorie_target,
        scan_bmr_kcal: scan.as_ref().and_then(|s| s.bmr_kcal),
        body_fat_pct: scan.as_ref().and_then(|s| s.body_fat_pct),
        adaptive_tdee: adaptive_tdee(
            usize::try_from(food_days).unwrap_or(0),
            total_kcal.unwrap_or(0.0),
            &weights,
        ),
    };
    Ok(compute(&inputs))
}

async fn get_targets(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Targets>, AppError> {
    Ok(Json(
        compute_for_user(&state.pool, user.0, Utc::now()).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 0.01, "{a} != {b}");
    }

    fn base() -> Inputs {
        Inputs {
            profile_target: 2200,
            ..Default::default()
        }
    }

    #[test]
    fn mifflin_male_and_female() {
        approx(mifflin_st_jeor(80.0, 180.0, 30, Sex::Male), 1780.0);
        approx(mifflin_st_jeor(60.0, 165.0, 25, Sex::Female), 1345.25);
    }

    #[test]
    fn katch_mcardle_uses_lean_mass() {
        approx(lean_mass(80.0, 25.0), 60.0);
        approx(katch_mcardle(60.0), 1666.0);
    }

    #[test]
    fn age_handles_birthday() {
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd).unwrap();
        assert_eq!(age_years(d(2000, 6, 15), d(2026, 6, 14)), 25);
        assert_eq!(age_years(d(2000, 6, 15), d(2026, 6, 15)), 26);
    }

    #[test]
    fn bmr_precedence() {
        let mut i = Inputs {
            weight_kg: Some(80.0),
            height_cm: Some(180.0),
            age: Some(30),
            sex: Some(Sex::Male),
            body_fat_pct: Some(25.0),
            scan_bmr_kcal: Some(1900),
            ..base()
        };
        assert_eq!(compute_bmr(&i), Some((1900.0, "inbody")));
        i.scan_bmr_kcal = None;
        let (b, s) = compute_bmr(&i).unwrap();
        approx(b, 1666.0);
        assert_eq!(s, "katch_mcardle");
        i.body_fat_pct = None;
        let (b, s) = compute_bmr(&i).unwrap();
        approx(b, 1780.0);
        assert_eq!(s, "mifflin");
        i.sex = None;
        assert_eq!(compute_bmr(&i), None);
    }

    #[test]
    fn activity_multipliers() {
        for (l, m) in [
            ("sedentary", 1.2),
            ("light", 1.375),
            ("moderate", 1.55),
            ("active", 1.725),
            ("very_active", 1.9),
        ] {
            approx(activity_multiplier(Some(l)), m);
        }
        approx(activity_multiplier(None), 1.55);
    }

    #[test]
    fn goal_deficit_and_surplus() {
        // 0.5 kg/week = 550 kcal/day
        assert_eq!(
            calorie_target(2600.0, Some("lose"), None, None, Some(Sex::Male)),
            2050
        );
        assert_eq!(
            calorie_target(2600.0, Some("lose"), Some(1.0), None, Some(Sex::Male)),
            1500
        );
        // 0.25 kg/week = 275 kcal/day -> 2875 rounds to 2880 (round half away)
        assert_eq!(calorie_target(2600.0, Some("gain"), None, None, None), 2880);
        assert_eq!(
            calorie_target(2634.0, Some("maintain"), None, None, None),
            2630
        );
        assert_eq!(calorie_target(2634.0, None, None, None, None), 2630);
        // explicit zero rate means no change
        assert_eq!(
            calorie_target(2600.0, Some("lose"), Some(0.0), None, None),
            2600
        );
    }

    #[test]
    fn floors_apply() {
        // sex floors
        assert_eq!(
            calorie_target(1500.0, Some("lose"), Some(1.0), None, Some(Sex::Female)),
            1200
        );
        assert_eq!(
            calorie_target(1700.0, Some("lose"), Some(1.0), None, Some(Sex::Male)),
            1500
        );
        assert_eq!(
            calorie_target(1500.0, Some("lose"), Some(1.0), None, None),
            1200
        );
        // BMR floor beats the sex floor
        assert_eq!(
            calorie_target(
                2000.0,
                Some("lose"),
                Some(1.0),
                Some(1783.0),
                Some(Sex::Male)
            ),
            1790
        );
    }

    #[test]
    fn macros_with_and_without_lean_mass() {
        // no body fat: protein 1.6 * 80 = 128; fat max(48, 0.27*2000/9 = 60) = 60
        // carbs = (2000 - 512 - 540) / 4 = 237
        assert_eq!(macros(2000, 80.0, None), (128, 60, 237));
        // lean mass 60: protein 120
        let (p, f, c) = macros(2000, 80.0, Some(60.0));
        assert_eq!((p, f), (120, 60));
        assert_eq!(c, 245);
    }

    #[test]
    fn macros_fat_floor_and_carbs_floor() {
        // fat floor 0.6 g/kg wins on a tiny target; carbs never negative
        let (p, f, c) = macros(600, 100.0, None);
        assert_eq!((p, f), (160, 60));
        assert_eq!(c, 0);
    }

    fn weights(days: &[(i64, f64)]) -> Vec<(DateTime<Utc>, f64)> {
        let now = Utc::now();
        days.iter()
            .map(|(d, kg)| (now - Duration::days(*d), *kg))
            .collect()
    }

    #[test]
    fn adaptive_needs_enough_data() {
        let w = weights(&[(20, 80.0), (1, 79.0)]);
        assert!(adaptive_tdee(13, 30000.0, &w).is_none());
        assert!(adaptive_tdee(14, 30000.0, &weights(&[(20, 80.0)])).is_none());
        // weights only 10 days apart
        assert!(adaptive_tdee(14, 30000.0, &weights(&[(12, 80.0), (2, 79.0)])).is_none());
        assert!(adaptive_tdee(14, 30000.0, &w).is_some());
    }

    #[test]
    fn adaptive_value() {
        // 20 days of 2000 kcal avg over 15 logged days; lost 1 kg over 19 days
        let w = weights(&[(20, 80.0), (1, 79.0)]);
        let t = adaptive_tdee(15, 15.0 * 2000.0, &w).unwrap();
        approx(t, 2000.0 + 7700.0 / 19.0);
        // gaining means expenditure below intake
        let w = weights(&[(20, 79.0), (1, 80.0)]);
        let t = adaptive_tdee(15, 15.0 * 2000.0, &w).unwrap();
        approx(t, 2000.0 - 7700.0 / 19.0);
    }

    #[test]
    fn blend_clamps_to_25_percent() {
        let (t, s) = blend_tdee(Some(2000.0), None).unwrap();
        approx(t, 2000.0);
        assert_eq!(s, "formula");
        let (t, s) = blend_tdee(Some(2000.0), Some(2400.0)).unwrap();
        approx(t, 2200.0);
        assert_eq!(s, "blended");
        let (t, _) = blend_tdee(Some(2000.0), Some(5000.0)).unwrap();
        approx(t, 2500.0);
        let (t, _) = blend_tdee(Some(2000.0), Some(100.0)).unwrap();
        approx(t, 1500.0);
        let (t, s) = blend_tdee(None, Some(2300.0)).unwrap();
        approx(t, 2300.0);
        assert_eq!(s, "adaptive");
        assert!(blend_tdee(None, None).is_none());
    }

    #[test]
    fn nothing_computable_falls_back_to_profile_value() {
        let t = compute(&base());
        assert_eq!(t.calorie_target, 2200);
        assert_eq!(t.calorie_target_source, "default");
        assert_eq!(t.bmr, None);
        assert_eq!(t.tdee, None);
        assert_eq!(t.protein_g, None);
        assert_eq!(
            t.missing,
            vec!["weight_kg", "height_cm", "birth_date", "sex"]
        );
    }

    #[test]
    fn full_mifflin_flow() {
        let t = compute(&Inputs {
            weight_kg: Some(80.0),
            height_cm: Some(180.0),
            age: Some(30),
            sex: Some(Sex::Male),
            activity_level: Some("moderate".into()),
            goal: Some("lose".into()),
            ..base()
        });
        // BMR 1780, TDEE 2759, minus 550 -> 2209 -> 2210
        assert_eq!(t.bmr, Some(1780));
        assert_eq!(t.bmr_source, Some("mifflin"));
        assert_eq!(t.tdee, Some(2759));
        assert_eq!(t.tdee_source, Some("formula"));
        assert_eq!(t.calorie_target, 2210);
        assert_eq!(t.calorie_target_source, "auto");
        assert!(t.missing.is_empty());
        assert_eq!(t.protein_g, Some(128));
        assert_eq!(t.lean_mass_kg, None);
    }

    #[test]
    fn inbody_scan_flow_uses_lean_mass_protein() {
        let t = compute(&Inputs {
            weight_kg: Some(80.0),
            body_fat_pct: Some(25.0),
            scan_bmr_kcal: Some(1800),
            activity_level: Some("sedentary".into()),
            goal: Some("maintain".into()),
            ..base()
        });
        assert_eq!(t.bmr_source, Some("inbody"));
        assert_eq!(t.tdee, Some(2160));
        assert_eq!(t.calorie_target, 2160);
        assert_eq!(t.lean_mass_kg, Some(60.0));
        assert_eq!(t.protein_g, Some(120));
        assert_eq!(t.body_fat_pct, Some(25.0));
    }

    #[test]
    fn manual_mode_keeps_profile_value_but_still_reports_macros() {
        let t = compute(&Inputs {
            weight_kg: Some(80.0),
            height_cm: Some(180.0),
            age: Some(30),
            sex: Some(Sex::Male),
            manual_mode: true,
            profile_target: 1800,
            goal: Some("lose".into()),
            ..base()
        });
        assert_eq!(t.calorie_target, 1800);
        assert_eq!(t.calorie_target_source, "manual");
        assert_eq!(t.tdee_source, Some("formula"));
        assert_eq!(t.protein_g, Some(128));
    }

    #[test]
    fn adaptive_only_when_no_formula() {
        let t = compute(&Inputs {
            adaptive_tdee: Some(2300.0),
            ..base()
        });
        assert_eq!(t.tdee_source, Some("adaptive"));
        assert_eq!(t.calorie_target, 2300);
        assert_eq!(t.calorie_target_source, "auto");
    }
}
