import type { LocalRow, OutboxItem, SyncDb, TableName } from './types';

/** In-memory SyncDb for jest. */
export function createFakeDb(): SyncDb {
  let rows = new Map<string, LocalRow>();
  let outbox = new Map<string, OutboxItem>();
  let meta = new Map<string, string>();
  const k = (t: string, id: string) => `${t}:${id}`;
  const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v)) as T;
  return {
    async getRow(t: TableName, id) {
      const r = rows.get(k(t, id));
      return r ? clone(r) : null;
    },
    async putRow(t, row) {
      rows.set(k(t, row.id), clone(row));
    },
    async listRows(t) {
      return [...rows.entries()].filter(([key]) => key.startsWith(`${t}:`)).map(([, v]) => clone(v));
    },
    async getOutboxItem(t, id) {
      const i = outbox.get(k(t, id));
      return i ? clone(i) : null;
    },
    async putOutboxItem(item) {
      outbox.set(k(item.table, item.id), clone(item));
    },
    async removeOutboxItem(t, id) {
      outbox.delete(k(t, id));
    },
    async listOutbox() {
      return [...outbox.values()].map(clone).sort((a, b) => a.updated_at.localeCompare(b.updated_at));
    },
    async getMeta(key) {
      return meta.get(key) ?? null;
    },
    async setMeta(key, value) {
      meta.set(key, value);
    },
    async clearAll() {
      rows = new Map();
      outbox = new Map();
      meta = new Map();
    },
  };
}
