import { authedRequest } from './client';

export type HealthCategory = 'steps' | 'workouts' | 'sleep';
export const HEALTH_CATEGORIES: readonly HealthCategory[] = ['steps', 'workouts', 'sleep'];

export type HealthConsents = Record<HealthCategory, boolean>;

export type StepItem = { external_id: string; date: string; count: number };
export type WorkoutItem = {
  external_id: string;
  started_at: string;
  duration_s: number;
  kind: string;
};
export type SleepItem = { external_id: string; bedtime: string; wake_time: string };

export type ImportBody = {
  steps?: StepItem[];
  workouts?: WorkoutItem[];
  sleep?: SleepItem[];
};

export type Counts = { imported: number; skipped: number; duplicates: number };
export type ImportResult = Record<HealthCategory, Counts>;

/** Server-side cap per category per request. */
export const MAX_ITEMS_PER_CATEGORY = 1000;

export function getConsents(): Promise<HealthConsents> {
  return authedRequest<HealthConsents>('/health/consents');
}

/** Sets all three consents. Revoking stops future imports but keeps data already imported. */
export function putConsents(consents: HealthConsents): Promise<HealthConsents> {
  return authedRequest<HealthConsents>('/health/consents', 'PUT', consents);
}

/** Idempotent: re-sending the same external ids reports them as duplicates. */
export function importHealth(body: ImportBody): Promise<ImportResult> {
  return authedRequest<ImportResult>('/health/import', 'POST', body);
}
