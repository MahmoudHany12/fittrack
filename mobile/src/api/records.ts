import { authedRequest } from './client';

export type RecordEntry = {
  exercise_id: string;
  exercise: string;
  heaviest_kg: number | null;
  heaviest_reps?: number | null;
  best_1rm_kg: number | null;
  achieved_at: string | null;
};

export const recordsKeys = { all: ['records'] as const };

export function getRecords(): Promise<RecordEntry[]> {
  return authedRequest<RecordEntry[] | { records: RecordEntry[] }>('/records').then((r) =>
    Array.isArray(r) ? r : (r?.records ?? []),
  );
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
