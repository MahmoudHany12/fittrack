//! Profile: display name, daily calorie target, weight goal and display units.
//! The row is created lazily on the first PUT; GET returns defaults until then.
use crate::{auth::AuthUser, error::AppError, extract::ValidatedJson, AppState};
use axum::{extract::State, routing::get, Json, Router};
use chrono::{DateTime, Utc};
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
    pub updated_at: Option<DateTime<Utc>>,
}

impl Profile {
    fn defaults() -> Self {
        Self {
            display_name: None,
            calorie_target: DEFAULT_CALORIE_TARGET,
            weight_goal_kg: None,
            units: "metric".into(),
            updated_at: None,
        }
    }
}

fn validate_units(units: &str) -> Result<(), ValidationError> {
    match units {
        "metric" | "imperial" => Ok(()),
        _ => Err(ValidationError::new("units")),
    }
}

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
}

const COLUMNS: &str = "display_name, calorie_target, weight_goal_kg::float8 AS weight_goal_kg, \
                       units, updated_at";

async fn get_profile(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Profile>, AppError> {
    let row = sqlx::query_as::<_, Profile>(&format!(
        "SELECT {COLUMNS} FROM profiles WHERE user_id = $1"
    ))
    .bind(user.0)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(row.unwrap_or_else(Profile::defaults)))
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
        "INSERT INTO profiles (user_id, display_name, calorie_target, weight_goal_kg, units) \
         VALUES ($1, $2, $3, $4::float8::numeric(5,2), $5) \
         ON CONFLICT (user_id) DO UPDATE SET \
           display_name = EXCLUDED.display_name, \
           calorie_target = EXCLUDED.calorie_target, \
           weight_goal_kg = EXCLUDED.weight_goal_kg, \
           units = EXCLUDED.units, \
           updated_at = now() \
         RETURNING {COLUMNS}"
    ))
    .bind(user.0)
    .bind(name)
    .bind(req.calorie_target)
    .bind(req.weight_goal_kg)
    .bind(req.units)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row))
}
