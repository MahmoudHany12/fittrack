//! Profile: display name, daily calorie target, weight goal, display units and
//! the optional body fields used for energy targets (see docs/body.md).
//! The row is created lazily on the first PUT; GET returns defaults until then.
use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

pub const DEFAULT_CALORIE_TARGET: i32 = 2200;

pub fn routes() -> Router<AppState> {
    Router::new().route("/profile", get(get_profile).put(put_profile))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Profile {
    pub display_name: Option<String>,
    pub calorie_target: i32,
    pub weight_goal_kg: Option<f64>,
    pub units: String,
    pub height_cm: Option<f64>,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub activity_level: Option<String>,
    pub goal: Option<String>,
    pub goal_rate_kg_per_week: Option<f64>,
    pub calorie_target_mode: String,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Profile {
    pub fn defaults() -> Self {
        Self {
            display_name: None,
            calorie_target: DEFAULT_CALORIE_TARGET,
            weight_goal_kg: None,
            units: "metric".into(),
            height_cm: None,
            birth_date: None,
            sex: None,
            activity_level: None,
            goal: None,
            goal_rate_kg_per_week: None,
            calorie_target_mode: "auto".into(),
            updated_at: None,
        }
    }
}

fn one_of(v: &str, allowed: &[&str], code: &'static str) -> Result<(), ValidationError> {
    if allowed.contains(&v) {
        Ok(())
    } else {
        Err(ValidationError::new(code))
    }
}

fn validate_units(units: &str) -> Result<(), ValidationError> {
    one_of(units, &["metric", "imperial"], "units")
}

fn validate_sex(v: &str) -> Result<(), ValidationError> {
    one_of(v, &["male", "female"], "sex")
}

fn validate_activity(v: &str) -> Result<(), ValidationError> {
    one_of(
        v,
        &["sedentary", "light", "moderate", "active", "very_active"],
        "activity_level",
    )
}

fn validate_goal(v: &str) -> Result<(), ValidationError> {
    one_of(v, &["lose", "maintain", "gain"], "goal")
}

fn validate_mode(v: &str) -> Result<(), ValidationError> {
    one_of(v, &["auto", "manual"], "calorie_target_mode")
}

fn validate_birth_date(d: &NaiveDate) -> Result<(), ValidationError> {
    let age = crate::targets::age_years(*d, Utc::now().date_naive());
    if (13..=100).contains(&age) {
        Ok(())
    } else {
        Err(ValidationError::new("birth_date"))
    }
}

/// The body fields are optional; omitting one leaves the stored value unchanged.
#[derive(Deserialize, Validate)]
pub struct UpdateProfile {
    #[validate(length(max = 50))]
    pub display_name: Option<String>,
    #[validate(range(min = 800, max = 10000))]
    pub calorie_target: i32,
    #[validate(range(min = 20.0, max = 400.0))]
    pub weight_goal_kg: Option<f64>,
    #[validate(custom(function = "validate_units"))]
    pub units: String,
    #[validate(range(min = 100.0, max = 250.0))]
    pub height_cm: Option<f64>,
    #[validate(custom(function = "validate_birth_date"))]
    pub birth_date: Option<NaiveDate>,
    #[validate(custom(function = "validate_sex"))]
    pub sex: Option<String>,
    #[validate(custom(function = "validate_activity"))]
    pub activity_level: Option<String>,
    #[validate(custom(function = "validate_goal"))]
    pub goal: Option<String>,
    #[validate(range(min = 0.0, max = 1.0))]
    pub goal_rate_kg_per_week: Option<f64>,
    #[validate(custom(function = "validate_mode"))]
    pub calorie_target_mode: Option<String>,
}

const COLUMNS: &str = "display_name, calorie_target, weight_goal_kg::float8 AS weight_goal_kg, \
                       units, height_cm, birth_date, sex, activity_level, goal, \
                       goal_rate_kg_per_week, calorie_target_mode, updated_at";

pub async fn load(pool: &sqlx::PgPool, user_id: uuid::Uuid) -> Result<Profile, sqlx::Error> {
    let row = sqlx::query_as::<_, Profile>(&format!(
        "SELECT {COLUMNS} FROM profiles WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or_else(Profile::defaults))
}

async fn get_profile(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Profile>, AppError> {
    Ok(Json(load(&state.pool, user.0).await?))
}

async fn put_profile(
    State(state): State<AppState>,
    user: AuthUser,
    ValidatedJson(req): ValidatedJson<UpdateProfile>,
) -> Result<Json<Profile>, AppError> {
    let name = req
        .display_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    let row = sqlx::query_as::<_, Profile>(&format!(
        "INSERT INTO profiles (user_id, display_name, calorie_target, weight_goal_kg, units, \
           height_cm, birth_date, sex, activity_level, goal, goal_rate_kg_per_week, \
           calorie_target_mode) \
         VALUES ($1, $2, $3, $4::float8::numeric(5,2), $5, $6, $7, $8, $9, $10, $11, \
                 COALESCE($12, 'auto')) \
         ON CONFLICT (user_id) DO UPDATE SET \
           display_name = EXCLUDED.display_name, \
           calorie_target = EXCLUDED.calorie_target, \
           weight_goal_kg = EXCLUDED.weight_goal_kg, \
           units = EXCLUDED.units, \
           height_cm = COALESCE($6, profiles.height_cm), \
           birth_date = COALESCE($7, profiles.birth_date), \
           sex = COALESCE($8, profiles.sex), \
           activity_level = COALESCE($9, profiles.activity_level), \
           goal = COALESCE($10, profiles.goal), \
           goal_rate_kg_per_week = COALESCE($11, profiles.goal_rate_kg_per_week), \
           calorie_target_mode = COALESCE($12, profiles.calorie_target_mode), \
           updated_at = now() \
         RETURNING {COLUMNS}"
    ))
    .bind(user.0)
    .bind(name)
    .bind(req.calorie_target)
    .bind(req.weight_goal_kg)
    .bind(req.units)
    .bind(req.height_cm)
    .bind(req.birth_date)
    .bind(req.sex)
    .bind(req.activity_level)
    .bind(req.goal)
    .bind(req.goal_rate_kg_per_week)
    .bind(req.calorie_target_mode)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row))
}
