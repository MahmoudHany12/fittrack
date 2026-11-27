import { authedRequest } from './client';

export type StatusKey = 'well_rested' | 'okay' | 'tired' | 'no_data';
export type Trend = 'up' | 'flat' | 'down';

export type StatEntry = { value: number | null; level: number | null; progress: number | null };

/** Contract: docs/spec.md "GET /dashboard". Sections with no data return null numbers. */
export type Dashboard = {
  name: string | null;
  status: { key: StatusKey; label: string };
  readiness?: { score: number | null; components?: Record<string, number | null> } | null;
  sleep: { minutes: number | null; quality: number | null; label: string | null } | null;
  diet: {
    kcal_eaten: number | null;
    kcal_target: number | null;
    label: string | null;
    protein_g_eaten?: number | null;
    protein_target_g?: number | null;
  } | null;
  exercise: {
    steps: number | null;
    workout_minutes_today: number | null;
    workout_count_today?: number | null;
  } | null;
  weight: { kg: number | null; trend: Trend | null } | null;
  body?: {
    weight_kg: number | null;
    trend: Trend | null;
    body_fat_pct: number | null;
    skeletal_muscle_kg: number | null;
    measured_at: string | null;
  } | null;
  workout_window?: { minutes_today: number; goal_minutes: number; fraction: number } | null;
  stats: { strength: StatEntry; cardio: StatEntry; focus: StatEntry } | null;
};

export function getDashboard(): Promise<Dashboard> {
  return authedRequest<Dashboard>('/dashboard');
}
