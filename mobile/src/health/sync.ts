import * as SecureStore from 'expo-secure-store';

import {
  HEALTH_CATEGORIES,
  importHealth,
  type HealthConsents,
  type ImportBody,
  type ImportResult,
} from '@/api/health';

import { buildBatches, mapSleep, mapSteps, mapWorkouts, startOfLocalDay } from './mapping';
import type { HealthProvider } from './types';

const LAST_SYNC_KEY = 'fittrack.health_last_sync';
/** First sync looks this far back. */
export const INITIAL_LOOKBACK_DAYS = 30;
/** Sleep sessions can start before `since`; re-read a night's worth (dedupe makes it safe). */
const SLEEP_OVERLAP_MS = 36 * 60 * 60 * 1000;

let memoryFallback: string | null = null;

export async function loadLastSync(): Promise<Date | null> {
  let raw: string | null = null;
  try {
    raw = await SecureStore.getItemAsync(LAST_SYNC_KEY);
  } catch {
    raw = memoryFallback;
  }
  if (!raw) return null;
  const d = new Date(raw);
  return Number.isNaN(d.getTime()) ? null : d;
}

export async function saveLastSync(d: Date): Promise<void> {
  const raw = d.toISOString();
  memoryFallback = raw;
  try {
    await SecureStore.setItemAsync(LAST_SYNC_KEY, raw);
  } catch {
    // keep the in-memory value; the next sync just re-reads a little more (idempotent)
  }
}

export async function clearLastSync(): Promise<void> {
  memoryFallback = null;
  try {
    await SecureStore.deleteItemAsync(LAST_SYNC_KEY);
  } catch {
    // ignore
  }
}

export type SyncSummary = {
  batches: number;
  totals: ImportResult;
};

const emptyTotals = (): ImportResult => ({
  steps: { imported: 0, skipped: 0, duplicates: 0 },
  workouts: { imported: 0, skipped: 0, duplicates: 0 },
  sleep: { imported: 0, skipped: 0, duplicates: 0 },
});

export type SyncDeps = {
  provider: HealthProvider;
  consents: HealthConsents;
  /** Last successful sync; null on the first run. */
  lastSync: Date | null;
  now: Date;
  send?: (body: ImportBody) => Promise<ImportResult>;
  batchSize?: number;
};

/**
 * Read everything since the last sync for the consented categories and post it in batches.
 * The caller stores `now` as the new last-sync time only when this resolves (a failed
 * batch throws, so the next run starts from the same point).
 */
export async function runSync(deps: SyncDeps): Promise<SyncSummary> {
  const { provider, consents, now } = deps;
  const send = deps.send ?? importHealth;
  const since = deps.lastSync ?? new Date(now.getTime() - INITIAL_LOOKBACK_DAYS * 86400000);

  const [steps, workouts, sleep] = await Promise.all([
    consents.steps
      ? // whole days only: a partial day would overwrite the stored total with a smaller one
        provider.readSteps(startOfLocalDay(since)).then((r) => mapSteps(r, now))
      : [],
    consents.workouts ? provider.readWorkouts(since).then((r) => mapWorkouts(r, now)) : [],
    consents.sleep
      ? provider
          .readSleep(new Date(since.getTime() - SLEEP_OVERLAP_MS))
          .then((r) => mapSleep(r, now))
      : [],
  ]);

  const totals = emptyTotals();
  const batches = buildBatches({ steps, workouts, sleep }, deps.batchSize);
  for (const body of batches) {
    const res = await send(body);
    for (const c of HEALTH_CATEGORIES) {
      totals[c].imported += res[c]?.imported ?? 0;
      totals[c].skipped += res[c]?.skipped ?? 0;
      totals[c].duplicates += res[c]?.duplicates ?? 0;
    }
  }
  return { batches: batches.length, totals };
}
