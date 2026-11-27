import { getDb } from './db';
import { triggerSync } from './engine';
import type { LocalRow, OutboxItem, SyncDb, TableName } from './types';

export type SetIn = { exercise_id: string; reps: number; weight_kg: number };

export type WorkoutSave = {
  id: string;
  started_at: string;
  duration_s: number;
  note?: string | null;
  sets: SetIn[];
};
export type SleepSave = { id: string; bedtime: string; wake_time: string; quality?: number | null };
export type WeightSave = { id: string; kg: number; measured_at?: string };
export type FoodLogSave = {
  id: string;
  food_id: string;
  servings: number;
  kcal: number;
  protein_g: number;
  carbs_g: number;
  fat_g: number;
  logged_at?: string;
  meal?: string | null;
};

/** Strictly newer than any existing local version so last-write-wins always favours this edit. */
async function nextStamp(db: SyncDb, table: TableName, id: string): Promise<string> {
  const existing = await db.getRow(table, id);
  const pending = await db.getOutboxItem(table, id);
  let t = Date.now();
  for (const prev of [existing?.updated_at, pending?.updated_at]) {
    if (prev) t = Math.max(t, Date.parse(prev) + 1);
  }
  return new Date(t).toISOString();
}

/** Write locally, replace any pending change for the same id with this (newest) one, then sync. */
async function writeChange(
  table: TableName,
  id: string,
  data: Record<string, unknown>,
  deleted: boolean,
): Promise<void> {
  const db = await getDb();
  const updated_at = await nextStamp(db, table, id);
  const prior = await db.getRow(table, id);
  const row: LocalRow = {
    id,
    updated_at,
    deleted,
    data: deleted ? (prior?.data ?? {}) : data,
  };
  await db.putRow(table, row);
  const item: OutboxItem = {
    table,
    id,
    updated_at,
    payload: deleted ? { id, updated_at, deleted: true } : { id, updated_at, deleted: false, ...data },
    failed: false,
    reason: null,
  };
  await db.putOutboxItem(item);
  triggerSync();
}

export function saveWorkout(w: WorkoutSave): Promise<void> {
  return writeChange(
    'workouts',
    w.id,
    { started_at: w.started_at, duration_s: w.duration_s, note: w.note ?? null, sets: w.sets },
    false,
  );
}

export function saveSleep(s: SleepSave): Promise<void> {
  return writeChange(
    'sleep',
    s.id,
    { bedtime: s.bedtime, wake_time: s.wake_time, quality: s.quality ?? null },
    false,
  );
}

export function saveWeight(w: WeightSave): Promise<void> {
  return writeChange(
    'weight',
    w.id,
    { kg: w.kg, measured_at: w.measured_at ?? new Date().toISOString() },
    false,
  );
}

export function saveFoodLog(f: FoodLogSave): Promise<void> {
  return writeChange(
    'food_logs',
    f.id,
    {
      food_id: f.food_id,
      servings: f.servings,
      kcal: f.kcal,
      protein_g: f.protein_g,
      carbs_g: f.carbs_g,
      fat_g: f.fat_g,
      logged_at: f.logged_at ?? new Date().toISOString(),
      meal: f.meal ?? null,
    },
    false,
  );
}

export const deleteWorkout = (id: string) => writeChange('workouts', id, {}, true);
export const deleteSleep = (id: string) => writeChange('sleep', id, {}, true);
export const deleteWeight = (id: string) => writeChange('weight', id, {}, true);
export const deleteFoodLog = (id: string) => writeChange('food_logs', id, {}, true);

/** Wipe local data (sign-out). */
export async function clearLocalData(): Promise<void> {
  const db = await getDb();
  await db.clearAll();
}
