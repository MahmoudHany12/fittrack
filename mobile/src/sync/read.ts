import { getDb } from './db';
import type { LocalRow, OutboxItem, SyncDb, TableName } from './types';

/** Which data column holds the moment an entry "happened" (used for `since` and ordering). */
export const TIME_KEY: Record<TableName, string> = {
  workouts: 'started_at',
  sleep: 'wake_time',
  weight: 'measured_at',
  food_logs: 'logged_at',
};

export type Entry = {
  id: string;
  updated_at: string;
  data: Record<string, unknown>;
  /** True while the change is still in the outbox (not yet confirmed by the server). */
  pending: boolean;
};

const ms = (s: unknown): number => {
  const n = typeof s === 'string' ? Date.parse(s) : NaN;
  return Number.isNaN(n) ? 0 : n;
};

/** When the entry happened; falls back to updated_at. */
export function entryTime(table: TableName, data: Record<string, unknown>, updated_at: string): number {
  return ms(data[TIME_KEY[table]]) || ms(updated_at);
}

async function localRows(
  db: SyncDb,
  table: TableName,
  opts: { since?: string | Date; includeDeleted?: boolean },
): Promise<LocalRow[]> {
  const since = opts.since ? new Date(opts.since).getTime() : null;
  const rows = await db.listRows(table);
  return rows
    .filter((r) => opts.includeDeleted || !r.deleted)
    .filter((r) => since === null || entryTime(table, r.data, r.updated_at) >= since)
    .sort((a, b) => entryTime(table, b.data, b.updated_at) - entryTime(table, a.data, a.updated_at));
}

/** Non-deleted local rows of a table, newest first, optionally only those that happened since a time. */
export async function listLocal(
  table: TableName,
  opts: { since?: string | Date; includeDeleted?: boolean } = {},
): Promise<LocalRow[]> {
  return localRows(await getDb(), table, opts);
}

/** Changes waiting to be pushed (rejected ones stay local but are not "waiting"). */
export function countPending(outbox: OutboxItem[]): number {
  return outbox.filter((o) => !o.failed).length;
}

export async function pendingCount(): Promise<number> {
  return countPending(await (await getDb()).listOutbox());
}

export async function pendingIds(table: TableName): Promise<Set<string>> {
  const out = await (await getDb()).listOutbox();
  return new Set(out.filter((o) => o.table === table && !o.failed).map((o) => o.id));
}

type ServerRow = { id: string; updated_at: string } & Record<string, unknown>;

/**
 * Merge server rows with local rows by id. A local row wins when its updated_at is newer
 * (or the server does not know it); a newer local delete hides the server row.
 * Result is newest-first by entry time.
 */
export function mergeEntries(
  table: TableName,
  server: ServerRow[],
  local: LocalRow[],
  pending: Set<string>,
): Entry[] {
  const byId = new Map<string, Entry>();
  for (const s of server) {
    const { id, updated_at, ...data } = s;
    byId.set(id, { id, updated_at, data, pending: false });
  }
  for (const l of local) {
    const cur = byId.get(l.id);
    if (cur && ms(cur.updated_at) > ms(l.updated_at)) continue;
    if (l.deleted) {
      byId.delete(l.id);
      continue;
    }
    byId.set(l.id, {
      id: l.id,
      updated_at: l.updated_at,
      data: { ...(cur?.data ?? {}), ...l.data },
      pending: pending.has(l.id),
    });
  }
  return [...byId.values()].sort(
    (a, b) => entryTime(table, b.data, b.updated_at) - entryTime(table, a.data, a.updated_at),
  );
}

export type PendingTotals = { kcal: number; minutes: number };

const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0);

/** kcal / workout minutes from still-pending (unsynced) food logs and workouts logged since `since`. */
export function pendingTotals(outbox: OutboxItem[], since: Date): PendingTotals {
  let kcal = 0;
  let seconds = 0;
  for (const o of outbox) {
    if (o.failed || o.payload.deleted === true) continue;
    if (o.table !== 'food_logs' && o.table !== 'workouts') continue;
    if (entryTime(o.table, o.payload, o.updated_at) < since.getTime()) continue;
    if (o.table === 'food_logs') kcal += num(o.payload.kcal);
    else seconds += num(o.payload.duration_s);
  }
  return { kcal: Math.round(kcal), minutes: Math.round(seconds / 60) };
}

export async function pendingTotalsSince(since: Date): Promise<PendingTotals> {
  return pendingTotals(await (await getDb()).listOutbox(), since);
}

/** Merged recent entries (server rows + local rows), newest first, capped. */
export async function recentEntries(table: TableName, server: ServerRow[], limit = 5): Promise<Entry[]> {
  const db = await getDb();
  const [local, pend] = await Promise.all([
    localRows(db, table, { includeDeleted: true }),
    pendingIds(table),
  ]);
  return mergeEntries(table, server, local, pend).slice(0, limit);
}
