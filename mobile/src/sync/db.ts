import type { LocalRow, OutboxItem, SyncDb, TableName } from './types';
import { TABLES } from './types';

type SqlDb = {
  execAsync(sql: string): Promise<void>;
  runAsync(sql: string, params?: unknown[]): Promise<unknown>;
  getFirstAsync<T>(sql: string, params?: unknown[]): Promise<T | null>;
  getAllAsync<T>(sql: string, params?: unknown[]): Promise<T[]>;
};

type RawRow = { id: string; updated_at: string; deleted: number; data: string };
type RawOutbox = {
  tbl: TableName;
  id: string;
  updated_at: string;
  payload: string;
  failed: number;
  reason: string | null;
};

const rowTable = (t: TableName) => `sync_${t}`; // always one of our own literals

const toRow = (r: RawRow): LocalRow => ({
  id: r.id,
  updated_at: r.updated_at,
  deleted: r.deleted === 1,
  data: JSON.parse(r.data) as Record<string, unknown>,
});

const toOutbox = (r: RawOutbox): OutboxItem => ({
  table: r.tbl,
  id: r.id,
  updated_at: r.updated_at,
  payload: JSON.parse(r.payload) as Record<string, unknown>,
  failed: r.failed === 1,
  reason: r.reason,
});

export function createSqliteDb(db: SqlDb): SyncDb {
  return {
    async getRow(table, id) {
      const r = await db.getFirstAsync<RawRow>(`SELECT * FROM ${rowTable(table)} WHERE id = ?`, [id]);
      return r ? toRow(r) : null;
    },
    async putRow(table, row) {
      await db.runAsync(
        `INSERT OR REPLACE INTO ${rowTable(table)} (id, updated_at, deleted, data) VALUES (?, ?, ?, ?)`,
        [row.id, row.updated_at, row.deleted ? 1 : 0, JSON.stringify(row.data)],
      );
    },
    async listRows(table) {
      const rows = await db.getAllAsync<RawRow>(`SELECT * FROM ${rowTable(table)}`);
      return rows.map(toRow);
    },
    async getOutboxItem(table, id) {
      const r = await db.getFirstAsync<RawOutbox>('SELECT * FROM outbox WHERE tbl = ? AND id = ?', [
        table,
        id,
      ]);
      return r ? toOutbox(r) : null;
    },
    async putOutboxItem(item) {
      await db.runAsync(
        'INSERT OR REPLACE INTO outbox (tbl, id, updated_at, payload, failed, reason) VALUES (?, ?, ?, ?, ?, ?)',
        [
          item.table,
          item.id,
          item.updated_at,
          JSON.stringify(item.payload),
          item.failed ? 1 : 0,
          item.reason,
        ],
      );
    },
    async removeOutboxItem(table, id) {
      await db.runAsync('DELETE FROM outbox WHERE tbl = ? AND id = ?', [table, id]);
    },
    async listOutbox() {
      const rows = await db.getAllAsync<RawOutbox>('SELECT * FROM outbox ORDER BY updated_at');
      return rows.map(toOutbox);
    },
    async getMeta(key) {
      const r = await db.getFirstAsync<{ value: string }>('SELECT value FROM meta WHERE key = ?', [key]);
      return r ? r.value : null;
    },
    async setMeta(key, value) {
      await db.runAsync('INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)', [key, value]);
    },
    async clearAll() {
      for (const t of TABLES) await db.runAsync(`DELETE FROM ${rowTable(t)}`);
      await db.runAsync('DELETE FROM outbox');
      await db.runAsync('DELETE FROM meta');
    },
  };
}

const SCHEMA = `
${TABLES.map(
  (t) =>
    `CREATE TABLE IF NOT EXISTS ${rowTable(t)} (id TEXT PRIMARY KEY NOT NULL, updated_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, data TEXT NOT NULL);`,
).join('\n')}
CREATE TABLE IF NOT EXISTS outbox (tbl TEXT NOT NULL, id TEXT NOT NULL, updated_at TEXT NOT NULL, payload TEXT NOT NULL, failed INTEGER NOT NULL DEFAULT 0, reason TEXT, PRIMARY KEY (tbl, id));
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
`;

let current: SyncDb | null = null;
let opening: Promise<SyncDb> | null = null;

/** Tests inject a fake; the app lazily opens the real database. */
export function setDb(db: SyncDb | null): void {
  current = db;
  opening = null;
}

export function getDb(): Promise<SyncDb> {
  if (current) return Promise.resolve(current);
  if (!opening) {
    opening = (async () => {
      // Lazy require keeps jest (which injects a fake) from loading the native module.
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const SQLite = require('expo-sqlite') as {
        openDatabaseAsync(name: string): Promise<SqlDb>;
      };
      const raw = await SQLite.openDatabaseAsync('fittrack-sync.db');
      await raw.execAsync(SCHEMA);
      current = createSqliteDb(raw);
      return current;
    })().catch((e) => {
      opening = null;
      throw e;
    });
  }
  return opening;
}
