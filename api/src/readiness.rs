//! Readiness score (0-100). See docs/readiness.md for the full definition.
//!
//! Pure functions only: callers load sleep/workouts and pass them in, which
//! keeps the maths unit-testable without a database.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub sleep: f64,
    pub training: f64,
    pub recovery: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            sleep: 0.5,
            training: 0.3,
            recovery: 0.2,
        }
    }
}

impl Weights {
    /// Overrides from `READINESS_W_SLEEP/TRAINING/RECOVERY`; invalid values fall back to defaults.
    pub fn from_env() -> Self {
        let d = Self::default();
        let get = |k: &str, def: f64| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(def)
        };
        Self {
            sleep: get("READINESS_W_SLEEP", d.sleep),
            training: get("READINESS_W_TRAINING", d.training),
            recovery: get("READINESS_W_RECOVERY", d.recovery),
        }
        .normalized()
    }

    /// Scale so the weights sum to 1 (defaults if they sum to 0).
    pub fn normalized(self) -> Self {
        let sum = self.sleep + self.training + self.recovery;
        if sum <= f64::EPSILON {
            return Self::default();
        }
        Self {
            sleep: self.sleep / sum,
            training: self.training / sum,
            recovery: self.recovery / sum,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SleepInput {
    pub minutes: i64,
    pub quality: Option<i16>,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkoutInput {
    pub started_at: DateTime<Utc>,
    pub duration_s: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    WellRested,
    Okay,
    Tired,
    NoData,
}

impl Status {
    pub fn from_score(score: Option<u8>) -> Self {
        match score {
            None => Status::NoData,
            Some(s) if s >= 70 => Status::WellRested,
            Some(s) if s >= 40 => Status::Okay,
            Some(_) => Status::Tired,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::WellRested => "Well rested",
            Status::Okay => "Okay",
            Status::Tired => "Tired",
            Status::NoData => "No data yet",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Components {
    pub sleep: Option<f64>,
    pub training: Option<f64>,
    pub recovery: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Readiness {
    pub score: Option<u8>,
    pub status: Status,
    pub components: Components,
}

fn lerp(x: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    y0 + (y1 - y0) * ((x - x0) / (x1 - x0))
}

/// 100 for 7.5-9 h, falling linearly to 0 at 3 h and to 60 at 11 h (then 60 -> 40 at 14 h, floor 40).
fn hours_score(hours: f64) -> f64 {
    let s = if hours < 3.0 {
        0.0
    } else if hours < 7.5 {
        lerp(hours, 3.0, 7.5, 0.0, 100.0)
    } else if hours <= 9.0 {
        100.0
    } else if hours <= 11.0 {
        lerp(hours, 9.0, 11.0, 100.0, 60.0)
    } else {
        60.0
    };
    s.clamp(0.0, 100.0)
}

pub fn sleep_score(s: &SleepInput) -> f64 {
    let hours = hours_score(s.minutes.max(0) as f64 / 60.0);
    match s.quality {
        Some(q) if (1..=5).contains(&q) => 0.8 * hours + 0.2 * (f64::from(q - 1) / 4.0 * 100.0),
        _ => hours,
    }
}

/// Training freshness from the last 7 days of workouts (recent days count more).
pub fn training_score(workouts: &[WorkoutInput], now: DateTime<Utc>) -> f64 {
    let mut load = 0.0;
    for w in workouts {
        let age = now - w.started_at;
        if age < Duration::zero() || age > Duration::days(7) {
            continue;
        }
        let minutes = w.duration_s.max(0) as f64 / 60.0;
        load += if age <= Duration::days(3) {
            minutes
        } else {
            minutes * 0.4
        };
    }
    if load <= 0.0 {
        return 85.0; // rested, slightly detrained
    }
    if load <= 90.0 {
        100.0
    } else if load >= 400.0 {
        20.0
    } else {
        lerp(load, 90.0, 400.0, 100.0, 20.0)
    }
}

pub fn recovery_score(
    workouts: &[WorkoutInput],
    sleep: Option<&SleepInput>,
    now: DateTime<Utc>,
) -> f64 {
    let last = workouts
        .iter()
        .filter(|w| w.started_at <= now)
        .map(|w| w.started_at.date_naive())
        .max();
    let mut score: f64 = match last {
        None => 100.0,
        Some(d) => match (now.date_naive() - d).num_days() {
            0 => 40.0,
            1 => 70.0,
            _ => 100.0,
        },
    };
    if let Some(s) = sleep {
        if s.minutes < 360 {
            score -= 15.0;
        }
    }
    score.clamp(0.0, 100.0)
}

pub fn compute(
    sleep: Option<&SleepInput>,
    workouts: &[WorkoutInput],
    now: DateTime<Utc>,
    weights: Weights,
) -> Readiness {
    let recent = workouts
        .iter()
        .any(|w| w.started_at <= now && now - w.started_at <= Duration::days(7));
    if sleep.is_none() && !recent {
        return Readiness {
            score: None,
            status: Status::NoData,
            components: Components {
                sleep: None,
                training: None,
                recovery: None,
            },
        };
    }
    let w = weights.normalized();
    let components = Components {
        sleep: sleep.map(sleep_score),
        training: Some(training_score(workouts, now)),
        recovery: Some(recovery_score(workouts, sleep, now)),
    };
    let parts = [
        (components.sleep, w.sleep),
        (components.training, w.training),
        (components.recovery, w.recovery),
    ];
    let total_w: f64 = parts
        .iter()
        .filter(|(v, _)| v.is_some())
        .map(|(_, w)| w)
        .sum();
    let value: f64 = parts
        .iter()
        .filter_map(|(v, w)| v.map(|v| v * w))
        .sum::<f64>()
        / total_w;
    let score = value.round().clamp(0.0, 100.0) as u8;
    Readiness {
        score: Some(score),
        status: Status::from_score(Some(score)),
        components,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap()
    }

    fn workout(hours_ago: i64, minutes: i64) -> WorkoutInput {
        WorkoutInput {
            started_at: now() - Duration::hours(hours_ago),
            duration_s: minutes * 60,
        }
    }

    fn sleep(hours: f64, q: Option<i16>) -> SleepInput {
        SleepInput {
            minutes: (hours * 60.0) as i64,
            quality: q,
        }
    }

    #[test]
    fn default_weights_are_50_30_20() {
        let w = Weights::default();
        assert_eq!((w.sleep, w.training, w.recovery), (0.5, 0.3, 0.2));
    }

    #[test]
    fn weights_are_renormalised() {
        let w = Weights {
            sleep: 5.0,
            training: 3.0,
            recovery: 2.0,
        }
        .normalized();
        assert!((w.sleep - 0.5).abs() < 1e-9 && (w.recovery - 0.2).abs() < 1e-9);
        assert_eq!(
            Weights {
                sleep: 0.0,
                training: 0.0,
                recovery: 0.0
            }
            .normalized(),
            Weights::default()
        );
    }

    #[test]
    fn sleep_scoring_curve() {
        assert_eq!(sleep_score(&sleep(8.0, None)), 100.0);
        assert_eq!(sleep_score(&sleep(2.0, None)), 0.0);
        assert!((sleep_score(&sleep(5.25, None)) - 50.0).abs() < 1e-6);
        assert!((sleep_score(&sleep(10.0, None)) - 80.0).abs() < 1e-6);
        // quality blends in 20%
        assert!((sleep_score(&sleep(8.0, Some(1))) - 80.0).abs() < 1e-6);
        assert!((sleep_score(&sleep(8.0, Some(5))) - 100.0).abs() < 1e-6);
        // out-of-range quality ignored
        assert_eq!(sleep_score(&sleep(8.0, Some(9))), 100.0);
    }

    #[test]
    fn training_load_scoring() {
        assert_eq!(training_score(&[], now()), 85.0);
        assert_eq!(training_score(&[workout(10, 60)], now()), 100.0);
        // 200 min in last 3 days: 100 - 80*(110/310)
        let s = training_score(&[workout(10, 100), workout(30, 100)], now());
        assert!((s - (100.0 - 80.0 * 110.0 / 310.0)).abs() < 1e-6);
        assert_eq!(training_score(&[workout(24, 500)], now()), 20.0);
        // older days weigh 0.4: 200 min * 0.4 = 80 -> 100
        assert_eq!(training_score(&[workout(24 * 5, 200)], now()), 100.0);
        // older than 7 days ignored
        assert_eq!(training_score(&[workout(24 * 9, 500)], now()), 85.0);
    }

    #[test]
    fn recovery_depends_on_days_since_last_workout() {
        assert_eq!(recovery_score(&[workout(1, 30)], None, now()), 40.0);
        assert_eq!(recovery_score(&[workout(18, 30)], None, now()), 70.0);
        assert_eq!(recovery_score(&[workout(60, 30)], None, now()), 100.0);
        assert_eq!(recovery_score(&[], None, now()), 100.0);
        let short = sleep(5.0, None);
        assert_eq!(recovery_score(&[], Some(&short), now()), 85.0);
    }

    #[test]
    fn no_data_gives_null_score() {
        let r = compute(None, &[], now(), Weights::default());
        assert_eq!(r.score, None);
        assert_eq!(r.status, Status::NoData);
        assert_eq!(r.status.label(), "No data yet");
    }

    #[test]
    fn full_score_uses_weights() {
        let s = sleep(8.0, None); // 100
        let w = [workout(60, 30)]; // training 100, recovery 100
        let r = compute(Some(&s), &w, now(), Weights::default());
        assert_eq!(r.score, Some(100));
        assert_eq!(r.status, Status::WellRested);
    }

    #[test]
    fn missing_sleep_renormalises_remaining_weights() {
        let w = [workout(1, 30)]; // training 100, recovery 40
        let r = compute(None, &w, now(), Weights::default());
        // (0.3*100 + 0.2*40) / 0.5 = 76
        assert_eq!(r.score, Some(76));
        assert!(r.components.sleep.is_none());
    }

    #[test]
    fn bad_night_after_hard_session_is_tired() {
        let s = sleep(3.5, Some(1));
        let w = [workout(2, 120), workout(26, 120), workout(50, 120)];
        let r = compute(Some(&s), &w, now(), Weights::default());
        assert_eq!(r.status, Status::Tired);
        assert!(r.score.unwrap() < 40);
    }

    #[test]
    fn status_thresholds() {
        assert_eq!(Status::from_score(Some(70)), Status::WellRested);
        assert_eq!(Status::from_score(Some(69)), Status::Okay);
        assert_eq!(Status::from_score(Some(40)), Status::Okay);
        assert_eq!(Status::from_score(Some(39)), Status::Tired);
    }
}
