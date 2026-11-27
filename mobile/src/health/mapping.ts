import {
  MAX_ITEMS_PER_CATEGORY,
  type ImportBody,
  type SleepItem,
  type StepItem,
  type WorkoutItem,
} from '@/api/health';

import type { SleepSegment, StepRecord, WorkoutRecord } from './types';

const MAX_STEPS_PER_DAY = 1_000_000;
const MAX_SLEEP_MS = 24 * 60 * 60 * 1000;
const MAX_WORKOUT_S = 86400;
/** Asleep segments closer than this belong to the same night. */
export const SLEEP_MERGE_GAP_MS = 60 * 60 * 1000;

const pad = (n: number) => String(n).padStart(2, '0');

/** Local calendar day as YYYY-MM-DD. */
export function localDay(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function startOfLocalDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

/** Sum step records per local day. One stable id per day, so a later, larger count updates it. */
export function mapSteps(records: readonly StepRecord[], now: Date): StepItem[] {
  const perDay = new Map<string, number>();
  for (const r of records) {
    if (!Number.isFinite(r.count) || r.count <= 0 || r.start.getTime() > now.getTime()) continue;
    const day = localDay(r.start);
    perDay.set(day, (perDay.get(day) ?? 0) + r.count);
  }
  return [...perDay.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .filter(([, count]) => Math.round(count) <= MAX_STEPS_PER_DAY)
    .map(([day, count]) => ({
      external_id: `steps-${day}`,
      date: day,
      count: Math.round(count),
    }));
}

export function mapWorkouts(records: readonly WorkoutRecord[], now: Date): WorkoutItem[] {
  const seen = new Set<string>();
  const out: WorkoutItem[] = [];
  for (const r of records) {
    const duration = Math.round((r.end.getTime() - r.start.getTime()) / 1000);
    const kind = r.kind.trim().slice(0, 40) || 'workout';
    if (!r.id || seen.has(r.id)) continue;
    if (duration < 1 || duration > MAX_WORKOUT_S || r.start.getTime() > now.getTime()) continue;
    seen.add(r.id);
    out.push({
      external_id: `workout-${r.id}`.slice(0, 200),
      started_at: r.start.toISOString(),
      duration_s: duration,
      kind,
    });
  }
  return out.sort((a, b) => a.started_at.localeCompare(b.started_at));
}

/**
 * Merge asleep segments into sleep sessions (gaps under an hour are joined). The id of a
 * session is derived from its first segment so repeated syncs produce the same id.
 * Sessions longer than 24 h are dropped (the API would reject them).
 */
export function mapSleep(segments: readonly SleepSegment[], now: Date): SleepItem[] {
  const asleep = segments
    .filter((s) => s.asleep && s.end.getTime() > s.start.getTime())
    .sort((a, b) => a.start.getTime() - b.start.getTime());
  const sessions: { firstId: string; start: Date; end: Date }[] = [];
  for (const s of asleep) {
    const last = sessions[sessions.length - 1];
    if (last && s.start.getTime() - last.end.getTime() <= SLEEP_MERGE_GAP_MS) {
      if (s.end.getTime() > last.end.getTime()) last.end = s.end;
    } else {
      sessions.push({ firstId: s.id, start: s.start, end: s.end });
    }
  }
  return sessions
    .filter((s) => s.end.getTime() - s.start.getTime() <= MAX_SLEEP_MS && s.end <= now)
    .map((s) => ({
      external_id: `sleep-${s.firstId}`.slice(0, 200),
      bedtime: s.start.toISOString(),
      wake_time: s.end.toISOString(),
    }));
}

export function chunk<T>(items: readonly T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}

/**
 * Split the items into request bodies. Batch i carries chunk i of every category, and no
 * category exceeds `size` (default: the server cap) in any one request.
 */
export function buildBatches(
  items: { steps: StepItem[]; workouts: WorkoutItem[]; sleep: SleepItem[] },
  size: number = MAX_ITEMS_PER_CATEGORY,
): ImportBody[] {
  const n = Math.min(size, MAX_ITEMS_PER_CATEGORY);
  const steps = chunk(items.steps, n);
  const workouts = chunk(items.workouts, n);
  const sleep = chunk(items.sleep, n);
  const count = Math.max(steps.length, workouts.length, sleep.length);
  const batches: ImportBody[] = [];
  for (let i = 0; i < count; i++) {
    const body: ImportBody = {};
    if (steps[i]) body.steps = steps[i];
    if (workouts[i]) body.workouts = workouts[i];
    if (sleep[i]) body.sleep = sleep[i];
    batches.push(body);
  }
  return batches;
}
