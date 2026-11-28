import { authedRequest } from './client';

export type RecordEntry = {
  exercise_id: string;
  exercise_name: string;
  best_weight_kg: number | null;
  best_weight_reps: number | null;
  best_weight_at: string | null;
  best_e1rm_kg: number | null;
  best_e1rm_weight_kg: number | null;
  best_e1rm_reps: number | null;
  best_e1rm_at: string | null;
};

export const recordsKeys = { all: ['records'] as const };

export function getRecords(): Promise<RecordEntry[]> {
  return authedRequest<RecordEntry[]>('/records');
}

const round1 = (n: number) => Math.round(n * 10) / 10;

export function kgText(n: number | null | undefined): string {
  return typeof n === 'number' && Number.isFinite(n) ? `${round1(n)} kg` : '-';
}

/** ISO timestamp -> local date text; empty for missing or invalid input. */
export function dateText(iso: string | null | undefined): string {
  if (!iso) return '';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString();
}

/** "100 kg × 5" or "100 kg" without reps. */
export function liftText(kg: number | null, reps: number | null): string {
  return `${kgText(kg)}${reps ? ` × ${reps}` : ''}`;
}
