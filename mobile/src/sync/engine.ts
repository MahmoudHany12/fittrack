import NetInfo from '@react-native-community/netinfo';
import { AppState, type AppStateStatus } from 'react-native';

import { authedRequest } from '@/api/client';

import { getDb } from './db';
import { TABLES, type LocalRow, type OutboxItem, type SyncDb, type TableName } from './types';

export const OVERLAP_MS = 5000;
export const PERIODIC_MS = 60_000;
export const MAX_BACKOFF_MS = 5 * 60_000;
const MAX_PUSH_PER_TABLE = 500;
const MAX_ROUNDS = 100;
const META_SERVER_TIME = 'last_server_time';

type PulledRow = { id: string; updated_at: string; deleted: boolean; synced_at?: string } & Record<
  string,
  unknown
>;

export type SyncResponse = {
  server_time: string;
  has_more: boolean;
  applied?: Record<string, { applied: number; ignored: number }>;
  rejected: { table: TableName; id: string; reason: string }[];
  changes: Partial<Record<TableName, PulledRow[]>>;
};

export type SyncResult = {
  ok: boolean;
  skipped?: boolean;
  pushed: number;
  pulled: number;
  failed: number;
};

type Listener = (r: SyncResult) => void;

let running: Promise<SyncResult> | null = null;
let again = false;
let failures = 0;
let backoffUntil = 0;
let retryTimer: ReturnType<typeof setTimeout> | null = null;
let failedCount = 0;
const listeners = new Set<Listener>();
const failedListeners = new Set<(n: number) => void>();

/** 1 s, 2 s, 4 s ... capped at five minutes. */
export function backoffDelay(consecutiveFailures: number): number {
  if (consecutiveFailures <= 0) return 0;
  return Math.min(MAX_BACKOFF_MS, 1000 * 2 ** (consecutiveFailures - 1));
}

export function onSynced(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Number of local changes the server rejected (they stay local and are never retried). */
export function getFailedCount(): number {
  return failedCount;
}

export function onFailedCountChange(fn: (n: number) => void): () => void {
  failedListeners.add(fn);
  return () => failedListeners.delete(fn);
}

function setFailedCount(n: number) {
  if (n === failedCount) return;
  failedCount = n;
  failedListeners.forEach((f) => f(n));
}

const ts = (s: string) => Date.parse(s);

async function applyPulled(db: SyncDb, table: TableName, rows: PulledRow[]): Promise<number> {
  let applied = 0;
  for (const r of rows) {
    const pending = await db.getOutboxItem(table, r.id);
    // A pending local edit that is at least as new wins; it will be pushed and decided by the server.
    if (pending && ts(pending.updated_at) >= ts(r.updated_at)) continue;
    const local = await db.getRow(table, r.id);
    if (local && !(ts(r.updated_at) > ts(local.updated_at))) continue;
    const { id, updated_at, deleted, synced_at, ...data } = r;
    const row: LocalRow = { id, updated_at, deleted: !!deleted, data };
    await db.putRow(table, row);
    applied++;
  }
  return applied;
}

async function runOnce(): Promise<SyncResult> {
  const db = await getDb();
  const result: SyncResult = { ok: true, pushed: 0, pulled: 0, failed: 0 };
  const stored = await db.getMeta(META_SERVER_TIME);
  let since: string | null = stored ? new Date(ts(stored) - OVERLAP_MS).toISOString() : null;

  for (let round = 0; round < MAX_ROUNDS; round++) {
    const outbox = (await db.listOutbox()).filter((i) => !i.failed);
    const sent: OutboxItem[] = [];
    const changes: Record<TableName, Record<string, unknown>[]> = {
      workouts: [],
      sleep: [],
      weight: [],
      food_logs: [],
    };
    for (const item of outbox) {
      if (changes[item.table].length >= MAX_PUSH_PER_TABLE) continue;
      changes[item.table].push(item.payload);
      sent.push(item);
    }

    const res = await authedRequest<SyncResponse>('/sync', 'POST', { since, changes });

    const rejectedKeys = new Set(res.rejected.map((r) => `${r.table}:${r.id}`));
    for (const r of res.rejected) {
      const cur = await db.getOutboxItem(r.table, r.id);
      const pushed = sent.find((s) => s.table === r.table && s.id === r.id);
      if (cur && pushed && cur.updated_at === pushed.updated_at) {
        console.warn(`sync: server rejected ${r.table}/${r.id}: ${r.reason}`);
        await db.putOutboxItem({ ...cur, failed: true, reason: r.reason });
        result.failed++;
      }
    }
    for (const s of sent) {
      if (rejectedKeys.has(`${s.table}:${s.id}`)) continue;
      // applied or ignored: either way the server has decided. Keep it if edited again mid-flight.
      const cur = await db.getOutboxItem(s.table, s.id);
      if (cur && cur.updated_at === s.updated_at) await db.removeOutboxItem(s.table, s.id);
      result.pushed++;
    }

    for (const table of TABLES) {
      result.pulled += await applyPulled(db, table, res.changes?.[table] ?? []);
    }

    if (res.has_more) {
      since = res.server_time; // no overlap when paging
      continue;
    }
    await db.setMeta(META_SERVER_TIME, res.server_time);
    // Anything still pending (beyond the per-table cap, or saved mid-flight) goes in another round.
    const remaining = (await db.listOutbox()).filter((i) => !i.failed);
    if (remaining.length === 0) break;
    since = new Date(ts(res.server_time) - OVERLAP_MS).toISOString();
  }

  setFailedCount((await db.listOutbox()).filter((i) => i.failed).length);
  return result;
}

function scheduleRetry() {
  if (retryTimer) clearTimeout(retryTimer);
  const delay = backoffDelay(failures);
  backoffUntil = Date.now() + delay;
  retryTimer = setTimeout(() => {
    retryTimer = null;
    void syncNow({ force: true });
  }, delay);
}

/**
 * Push pending changes and pull server changes. Single-flight: a call while one is running joins it
 * (and makes it loop once more so changes saved mid-flight are not stranded). While backing off after
 * a failure plain calls are skipped; `force` (foreground / connectivity regained) bypasses the wait.
 */
export function syncNow(opts: { force?: boolean } = {}): Promise<SyncResult> {
  if (running) {
    again = true;
    return running;
  }
  if (!opts.force && Date.now() < backoffUntil) {
    return Promise.resolve({ ok: false, skipped: true, pushed: 0, pulled: 0, failed: 0 });
  }
  if (opts.force && retryTimer) {
    clearTimeout(retryTimer);
    retryTimer = null;
  }
  running = (async () => {
    let result: SyncResult;
    try {
      do {
        again = false;
        result = await runOnce();
      } while (again);
      failures = 0;
      backoffUntil = 0;
      listeners.forEach((l) => l(result));
    } catch {
      failures++;
      scheduleRetry();
      result = { ok: false, pushed: 0, pulled: 0, failed: failedCount };
    } finally {
      running = null;
    }
    return result;
  })();
  return running;
}

// ---------- lifecycle ----------

let started = false;
let unsubNet: (() => void) | null = null;
let appSub: { remove: () => void } | null = null;
let interval: ReturnType<typeof setInterval> | null = null;
let appState: AppStateStatus = 'active';

/** Fire-and-forget attempt after a local write. No-op until the engine is started (signed in). */
export function triggerSync(): void {
  if (!started) return;
  void syncNow().catch(() => undefined);
}

export function startEngine(): void {
  if (started) return;
  started = true;
  appState = AppState.currentState === 'background' ? 'background' : 'active';
  let connected: boolean | null = null;

  appSub = AppState.addEventListener('change', (next) => {
    const wasBackground = appState !== 'active';
    appState = next;
    if (next === 'active' && wasBackground) void syncNow({ force: true });
  });
  unsubNet = NetInfo.addEventListener((s) => {
    const online = !!s.isConnected && s.isInternetReachable !== false;
    const regained = connected === false && online;
    connected = online;
    if (regained) void syncNow({ force: true });
  });
  interval = setInterval(() => {
    if (appState === 'active') void syncNow();
  }, PERIODIC_MS);
  void syncNow({ force: true });
}

export function stopEngine(): void {
  started = false;
  appSub?.remove();
  appSub = null;
  unsubNet?.();
  unsubNet = null;
  if (interval) clearInterval(interval);
  interval = null;
  if (retryTimer) clearTimeout(retryTimer);
  retryTimer = null;
  failures = 0;
  backoffUntil = 0;
  setFailedCount(0);
}

/** Test helper. */
export function _resetEngineForTests(): void {
  stopEngine();
  running = null;
  again = false;
}
