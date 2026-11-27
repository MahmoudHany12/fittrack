import { setDb } from './db';
import { createFakeDb } from './fakeDb';
import {
  countPending,
  listLocal,
  mergeEntries,
  pendingCount,
  pendingTotals,
  recentEntries,
} from './read';
import { deleteWeight, saveFoodLog, saveWeight, saveWorkout } from './store';
import type { LocalRow, OutboxItem } from './types';

jest.mock('./engine', () => ({ triggerSync: jest.fn() }));
jest.mock('@/api/client', () => ({ authedRequest: jest.fn() }));

const row = (id: string, updated_at: string, data: Record<string, unknown>, deleted = false): LocalRow => ({
  id,
  updated_at,
  deleted,
  data,
});
const outbox = (
  table: OutboxItem['table'],
  id: string,
  payload: Record<string, unknown>,
  failed = false,
): OutboxItem => ({
  table,
  id,
  updated_at: '2026-01-01T10:00:00.000Z',
  payload,
  failed,
  reason: null,
});

describe('mergeEntries', () => {
  const server = [
    { id: 'a', updated_at: '2026-01-01T10:00:00Z', kg: 80, measured_at: '2026-01-01T08:00:00Z' },
    { id: 'b', updated_at: '2026-01-02T10:00:00Z', kg: 79, measured_at: '2026-01-02T08:00:00Z' },
  ];

  it('keeps server rows and adds unknown local rows as pending, newest first', () => {
    const local = [row('c', '2026-01-03T10:00:00Z', { kg: 78, measured_at: '2026-01-03T08:00:00Z' })];
    const m = mergeEntries('weight', server, local, new Set(['c']));
    expect(m.map((e) => e.id)).toEqual(['c', 'b', 'a']);
    expect(m[0].pending).toBe(true);
    expect(m[1].pending).toBe(false);
  });

  it('local wins when newer, server wins when newer', () => {
    const newer = row('a', '2026-01-05T00:00:00Z', { kg: 75, measured_at: '2026-01-01T08:00:00Z' });
    const older = row('b', '2026-01-01T00:00:00Z', { kg: 1, measured_at: '2026-01-02T08:00:00Z' });
    const m = mergeEntries('weight', server, [newer, older], new Set(['a']));
    expect(m.find((e) => e.id === 'a')?.data.kg).toBe(75);
    expect(m.find((e) => e.id === 'a')?.pending).toBe(true);
    expect(m.find((e) => e.id === 'b')?.data.kg).toBe(79);
  });

  it('a newer local delete hides the server row', () => {
    const del = row('a', '2026-01-05T00:00:00Z', {}, true);
    expect(mergeEntries('weight', server, [del], new Set()).map((e) => e.id)).toEqual(['b']);
  });
});

describe('pending helpers', () => {
  it('countPending ignores rejected items', () => {
    expect(countPending([outbox('weight', 'a', {}), outbox('weight', 'b', {}, true)])).toBe(1);
  });

  it('pendingTotals sums only pending, non-deleted, today entries', () => {
    const since = new Date('2026-01-01T00:00:00Z');
    const items = [
      outbox('food_logs', 'f1', { kcal: 200, logged_at: '2026-01-01T12:00:00Z' }),
      outbox('food_logs', 'f2', { kcal: 100, logged_at: '2025-12-31T12:00:00Z' }),
      outbox('food_logs', 'f3', { kcal: 50, logged_at: '2026-01-01T12:00:00Z', deleted: true }),
      outbox('workouts', 'w1', { duration_s: 1800, started_at: '2026-01-01T07:00:00Z' }),
      outbox('workouts', 'w2', { duration_s: 600, started_at: '2026-01-01T07:00:00Z' }, true),
      outbox('weight', 'x', { kg: 80 }),
    ];
    expect(pendingTotals(items, since)).toEqual({ kcal: 200, minutes: 30 });
  });
});

describe('local reads through the store', () => {
  beforeEach(() => setDb(createFakeDb()));
  afterAll(() => setDb(null));

  it('listLocal returns non-deleted rows and honours since', async () => {
    await saveWeight({ id: 'w1', kg: 80, measured_at: '2026-01-01T08:00:00Z' });
    await saveWeight({ id: 'w2', kg: 79, measured_at: '2026-01-03T08:00:00Z' });
    await saveWeight({ id: 'w3', kg: 78, measured_at: '2026-01-04T08:00:00Z' });
    await deleteWeight('w3');
    expect((await listLocal('weight')).map((r) => r.id)).toEqual(['w2', 'w1']);
    expect((await listLocal('weight', { since: '2026-01-02T00:00:00Z' })).map((r) => r.id)).toEqual(['w2']);
    expect(await pendingCount()).toBe(3);
  });

  it('recentEntries merges and flags pending, capped to the limit', async () => {
    await saveFoodLog({
      id: 'f1',
      food_id: 'x',
      servings: 1,
      kcal: 250,
      protein_g: 1,
      carbs_g: 1,
      fat_g: 1,
      logged_at: '2026-01-02T12:00:00Z',
    });
    await saveWorkout({ id: 'w', started_at: '2026-01-02T08:00:00Z', duration_s: 60, sets: [] });
    const server = [
      { id: 's', updated_at: '2026-01-01T00:00:00Z', kcal: 100, logged_at: '2026-01-01T12:00:00Z' },
    ];
    const m = await recentEntries('food_logs', server, 5);
    expect(m.map((e) => [e.id, e.pending])).toEqual([
      ['f1', true],
      ['s', false],
    ]);
    expect(await recentEntries('food_logs', server, 1)).toHaveLength(1);
  });
});
