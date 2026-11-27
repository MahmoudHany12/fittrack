export const TABLES = ['workouts', 'sleep', 'weight', 'food_logs'] as const;
export type TableName = (typeof TABLES)[number];

/** A local mirror of a server row. `data` holds the table specific columns (and workout sets). */
export type LocalRow = {
  id: string;
  updated_at: string;
  deleted: boolean;
  data: Record<string, unknown>;
};

/** A pending change in wire format (what POST /sync expects), keyed by (table, id). */
export type OutboxItem = {
  table: TableName;
  id: string;
  updated_at: string;
  payload: Record<string, unknown>;
  failed: boolean;
  reason: string | null;
};

/** Minimal persistence surface so jest can use an in-memory fake instead of expo-sqlite. */
export interface SyncDb {
  getRow(table: TableName, id: string): Promise<LocalRow | null>;
  putRow(table: TableName, row: LocalRow): Promise<void>;
  listRows(table: TableName): Promise<LocalRow[]>;
  getOutboxItem(table: TableName, id: string): Promise<OutboxItem | null>;
  /** Insert or replace by (table, id). */
  putOutboxItem(item: OutboxItem): Promise<void>;
  removeOutboxItem(table: TableName, id: string): Promise<void>;
  listOutbox(): Promise<OutboxItem[]>;
  getMeta(key: string): Promise<string | null>;
  setMeta(key: string, value: string): Promise<void>;
  /** Delete every row, outbox item and meta value (sign-out). */
  clearAll(): Promise<void>;
}
