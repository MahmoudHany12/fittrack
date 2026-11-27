//! Data export: the authenticated user's own data as one JSON document, or a
//! single table as CSV. Password hash and tokens are never selected, and
//! soft-deleted rows are excluded. Every query filters by the user id.
use crate::{auth::AuthUser, error::AppError, AppState};
use axum::{
    extract::{Query, State},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/export", get(export))
}

#[derive(Deserialize)]
struct ExportQuery {
    format: Option<String>,
    table: Option<String>,
}

#[derive(Clone, Copy)]
enum Table {
    Workouts,
    Sets,
    Food,
    Sleep,
    Weight,
}

impl Table {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "workouts" => Self::Workouts,
            "sets" => Self::Sets,
            "food" => Self::Food,
            "sleep" => Self::Sleep,
            "weight" => Self::Weight,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Workouts => "workouts",
            Self::Sets => "sets",
            Self::Food => "food",
            Self::Sleep => "sleep",
            Self::Weight => "weight",
        }
    }

    /// Column order for CSV; also the keys of each JSON row.
    fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Workouts => &[
                "id",
                "started_at",
                "duration_s",
                "note",
                "source",
                "external_id",
                "updated_at",
            ],
            Self::Sets => &[
                "id",
                "workout_id",
                "exercise_id",
                "exercise",
                "reps",
                "weight_kg",
                "position",
            ],
            Self::Food => &[
                "id",
                "food_id",
                "name",
                "logged_at",
                "servings",
                "kcal",
                "protein_g",
                "carbs_g",
                "fat_g",
                "meal",
                "updated_at",
            ],
            Self::Sleep => &[
                "id",
                "bedtime",
                "wake_time",
                "quality",
                "source",
                "external_id",
                "updated_at",
            ],
            Self::Weight => &["id", "measured_at", "kg", "updated_at"],
        }
    }

    /// Inner query; `$1` is the user id. Rows are turned into JSON by the caller.
    fn sql(self) -> &'static str {
        match self {
            Self::Workouts => {
                "SELECT id, started_at, duration_s, note, source, external_id, updated_at \
                 FROM workouts WHERE user_id = $1 AND deleted_at IS NULL \
                 ORDER BY started_at, id"
            }
            Self::Sets => {
                "SELECT s.id, s.workout_id, s.exercise_id, e.name AS exercise, s.reps, \
                        s.weight_kg, s.position \
                 FROM workout_sets s \
                 JOIN workouts w ON w.id = s.workout_id \
                 JOIN exercises e ON e.id = s.exercise_id \
                 WHERE w.user_id = $1 AND w.deleted_at IS NULL \
                 ORDER BY w.started_at, w.id, s.position, s.id"
            }
            Self::Food => {
                "SELECT l.id, l.food_id, f.name, l.logged_at, l.servings, l.kcal, l.protein_g, \
                        l.carbs_g, l.fat_g, l.meal, l.updated_at \
                 FROM food_logs l JOIN foods f ON f.id = l.food_id \
                 WHERE l.user_id = $1 AND l.deleted_at IS NULL \
                 ORDER BY l.logged_at, l.id"
            }
            Self::Sleep => {
                "SELECT id, bedtime, wake_time, quality, source, external_id, updated_at \
                 FROM sleep_logs WHERE user_id = $1 AND deleted_at IS NULL \
                 ORDER BY wake_time, id"
            }
            Self::Weight => {
                "SELECT id, measured_at, kg, updated_at \
                 FROM weight_logs WHERE user_id = $1 AND deleted_at IS NULL \
                 ORDER BY measured_at, id"
            }
        }
    }
}

fn invalid(field: &str, code: &str) -> AppError {
    AppError::Validation(BTreeMap::from([(
        field.to_string(),
        vec![code.to_string()],
    )]))
}

async fn export(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ExportQuery>,
) -> Result<Response, AppError> {
    let format = q.format.as_deref().unwrap_or("json");
    match format {
        "json" => {
            if q.table.is_some() {
                return Err(invalid("table", "only_for_csv"));
            }
            let doc = export_json(&state.pool, user.0).await?;
            let body = serde_json::to_vec_pretty(&doc).map_err(|e| AppError::Internal(e.into()))?;
            Ok(attachment("application/json", "fittrack-export.json", body))
        }
        "csv" => {
            let table = q
                .table
                .as_deref()
                .ok_or_else(|| invalid("table", "required"))?;
            let table = Table::parse(table).ok_or_else(|| invalid("table", "unknown_table"))?;
            let rows = fetch_rows(&state.pool, user.0, table).await?;
            let csv = to_csv(table.columns(), &rows);
            Ok(attachment(
                "text/csv; charset=utf-8",
                &format!("fittrack-{}.csv", table.name()),
                csv.into_bytes(),
            ))
        }
        _ => Err(invalid("format", "unknown_format")),
    }
}

fn attachment(content_type: &'static str, filename: &str, body: Vec<u8>) -> Response {
    let mut resp = body.into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    // `filename` is built from fixed ASCII names only.
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp
}

/// Rows of `sql` as JSON objects. Postgres builds the JSON, which keeps types
/// (timestamps as RFC 3339, numerics as numbers) without per-column decoding.
async fn json_rows(pool: &PgPool, sql: &str, user_id: Uuid) -> Result<Vec<Value>, AppError> {
    let wrapped = format!("SELECT to_jsonb(t)::text FROM ({sql}) t");
    let rows: Vec<(String,)> = sqlx::query_as(&wrapped)
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|(s,)| serde_json::from_str(&s).map_err(|e| AppError::Internal(e.into())))
        .collect()
}

async fn fetch_rows(pool: &PgPool, user_id: Uuid, table: Table) -> Result<Vec<Value>, AppError> {
    json_rows(pool, table.sql(), user_id).await
}

async fn export_json(pool: &PgPool, user_id: Uuid) -> Result<Value, AppError> {
    let user = json_rows(
        pool,
        "SELECT id, email, created_at FROM users WHERE id = $1",
        user_id,
    )
    .await?
    .into_iter()
    .next()
    .ok_or(AppError::Unauthorized)?;
    let profile = json_rows(
        pool,
        "SELECT display_name, calorie_target, weight_goal_kg, units, updated_at \
         FROM profiles WHERE user_id = $1",
        user_id,
    )
    .await?
    .into_iter()
    .next();

    let mut workouts = fetch_rows(pool, user_id, Table::Workouts).await?;
    let sets = fetch_rows(pool, user_id, Table::Sets).await?;
    let mut by_workout: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for mut s in sets {
        if let Some(Value::String(w)) = s.get("workout_id").cloned() {
            if let Some(o) = s.as_object_mut() {
                o.remove("workout_id");
            }
            by_workout.entry(w).or_default().push(s);
        }
    }
    for w in &mut workouts {
        let id = w
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(o) = w.as_object_mut() {
            o.insert(
                "sets".into(),
                Value::Array(by_workout.remove(&id).unwrap_or_default()),
            );
        }
    }

    // The table is created by the health-import migration; tolerate its absence.
    let has_consents: (bool,) =
        sqlx::query_as("SELECT to_regclass('public.health_consents') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    let consents = if has_consents.0 {
        json_rows(
            pool,
            "SELECT category, granted, updated_at FROM health_consents WHERE user_id = $1 \
             ORDER BY category",
            user_id,
        )
        .await?
    } else {
        Vec::new()
    };

    let mut doc = Map::new();
    doc.insert("exported_at".into(), json!(chrono::Utc::now()));
    doc.insert("user".into(), user);
    doc.insert("profile".into(), profile.unwrap_or(Value::Null));
    doc.insert("workouts".into(), Value::Array(workouts));
    doc.insert(
        "food_logs".into(),
        Value::Array(fetch_rows(pool, user_id, Table::Food).await?),
    );
    doc.insert(
        "sleep_logs".into(),
        Value::Array(fetch_rows(pool, user_id, Table::Sleep).await?),
    );
    doc.insert(
        "weight_logs".into(),
        Value::Array(fetch_rows(pool, user_id, Table::Weight).await?),
    );
    doc.insert("health_consents".into(), Value::Array(consents));
    Ok(Value::Object(doc))
}

// ---------- CSV ----------

/// One CSV cell: neutralise spreadsheet formulas, then quote when needed.
pub fn csv_cell(raw: &str) -> String {
    let mut s = raw.to_string();
    if matches!(s.chars().next(), Some('=' | '+' | '-' | '@' | '\t' | '\r')) {
        s.insert(0, '\'');
    }
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn value_text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

pub fn to_csv(columns: &[&str], rows: &[Value]) -> String {
    let mut out = columns.join(",");
    out.push_str("\r\n");
    for row in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| csv_cell(&value_text(row.get(*c))))
            .collect();
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutralises_formulas() {
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("+1"), "'+1");
        assert_eq!(csv_cell("-2"), "'-2");
        assert_eq!(csv_cell("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_cell("a=b"), "a=b");
    }

    #[test]
    fn quotes_when_needed() {
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("two\nlines"), "\"two\nlines\"");
        assert_eq!(csv_cell("=A,B"), "\"'=A,B\"");
    }

    #[test]
    fn renders_rows() {
        let rows = vec![json!({"a": "x", "b": null, "c": 1.5}), json!({"a": "=y"})];
        assert_eq!(
            to_csv(&["a", "b", "c"], &rows),
            "a,b,c\r\nx,,1.5\r\n'=y,,\r\n"
        );
    }
}
