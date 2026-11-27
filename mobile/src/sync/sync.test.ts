import { authedRequest } from '@/api/client';

import { setDb } from './db';
import {
  _resetEngineForTests,
  backoffDelay,
  getFailedCount,
  startEngine,
  syncNow,
  type SyncResponse,
} from './engine';
import { createFakeDb } from './fakeDb';
import {
  clearLocalData,
  deleteSleep,
  deleteWeight,
  saveFoodLog,
  saveSleep,
  saveWeight,
  saveWorkout,
} from './store';
import type { SyncDb } from './types';

jest.mock('@/api/client', () => ({ authedRequest: jest.fn() }));

type NetListener = (s: { isConnected: boolean; isInternetReachable: boolean }) => void;
let netListener: NetListener | null = null;
jest.mock('@react-native-community/netinfo', () => ({
  __esModule: true,
  default: {
    addEventListener: (fn: NetListener) => {
      netListener = fn;
      return () => {
        netListener = null;
      };
    },
  },
}));

const mockRequest = authedRequest as jest.Mock;
let db: SyncDb;

const T0 = '2026-01-01T10:00:00.000Z';
const empty = (over: Partial<SyncResponse> = {}): SyncResponse => ({
  server_time: '2026-01-01T12:00:00.000Z',
  has_more: false,
  rejected: [],
  changes: {},
  ...over,
});

const ID = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const sleepIn = (id: string, quality = 3) => ({
  id,
  bedtime: '2026-01-01T22:00:00.000Z',
  wake_time: '2026-01-02T06:00:00.000Z',
  quality,
});

beforeEach(() => {
  jest.useFakeTimers({ now: new Date('2026-01-01T12:00:00.000Z') });
  db = createFakeDb();
  setDb(db);
  _resetEngineForTests();
  mockRequest.mockReset();
  jest.spyOn(console, 'warn').mockImplementation(() => undefined);
});

afterEach(() => {
  _resetEngineForTests();
  jest.useRealTimers();
});

describe('store + outbox', () => {
  it('works with no engine/network and coalesces pending changes per id keeping the newest', async () => {
    await saveSleep(sleepIn(ID(1), 2));
    await saveSleep(sleepIn(ID(1), 5));
    await saveWeight({ id: ID(2), kg: 80 });
    const outbox = await db.listOutbox();
    expect(outbox).toHaveLength(2);
    const s = outbox.find((o) => o.id === ID(1));
    expect(s?.payload.quality).toBe(5);
    expect((await db.getRow('sleep', ID(1)))?.data.quality).toBe(5);
    expect(mockRequest).not.toHaveBeenCalled();
  });

  it('always stamps a strictly newer updated_at, even within the same millisecond', async () => {
    await saveSleep(sleepIn(ID(1), 2));
    const a = (await db.getRow('sleep', ID(1)))?.updated_at as string;
    await saveSleep(sleepIn(ID(1), 3));
    const b = (await db.getRow('sleep', ID(1)))?.updated_at as string;
    expect(Date.parse(b)).toBeGreaterThan(Date.parse(a));
  });

  it('delete writes a tombstone and replaces a pending create', async () => {
    await saveWeight({ id: ID(2), kg: 80 });
    await deleteWeight(ID(2));
    const row = await db.getRow('weight', ID(2));
    expect(row?.deleted).toBe(true);
    const outbox = await db.listOutbox();
    expect(outbox).toHaveLength(1);
    expect(outbox[0].payload).toMatchObject({ id: ID(2), deleted: true });
  });

  it('builds wire payloads for workouts and food logs', async () => {
    await saveWorkout({
      id: ID(3),
      started_at: T0,
      duration_s: 600,
      sets: [{ exercise_id: ID(9), reps: 5, weight_kg: 50 }],
    });
    await saveFoodLog({
      id: ID(4),
      food_id: ID(8),
      servings: 2,
      kcal: 200,
      protein_g: 10,
      carbs_g: 20,
      fat_g: 5,
    });
    const out = await db.listOutbox();
    expect(out.find((o) => o.table === 'workouts')?.payload).toMatchObject({ duration_s: 600, deleted: false });
    expect(out.find((o) => o.table === 'food_logs')?.payload).toMatchObject({ servings: 2, kcal: 200 });
  });

  it('clearLocalData wipes rows, outbox and meta', async () => {
    await saveWeight({ id: ID(2), kg: 80 });
    await db.setMeta('last_server_time', T0);
    await clearLocalData();
    expect(await db.listOutbox()).toHaveLength(0);
    expect(await db.getRow('weight', ID(2))).toBeNull();
    expect(await db.getMeta('last_server_time')).toBeNull();
  });
});

describe('push', () => {
  it('offline save, then reconnect pushes the outbox and clears it', async () => {
    startEngine(); // initial sync fails: offline
    mockRequest.mockRejectedValueOnce(new Error('network'));
    await jest.advanceTimersByTimeAsync(0);
    mockRequest.mockReset();
    mockRequest.mockRejectedValue(new Error('network'));
    await saveSleep(sleepIn(ID(1)));
    await jest.advanceTimersByTimeAsync(0);
    expect(await db.listOutbox()).toHaveLength(1);

    mockRequest.mockReset();
    mockRequest.mockResolvedValue(empty());
    netListener?.({ isConnected: false, isInternetReachable: false });
    netListener?.({ isConnected: true, isInternetReachable: true });
    await jest.advanceTimersByTimeAsync(0);

    expect(mockRequest).toHaveBeenCalledWith(
      '/sync',
      'POST',
      expect.objectContaining({
        changes: expect.objectContaining({ sleep: [expect.objectContaining({ id: ID(1), quality: 3 })] }),
      }),
    );
    expect(await db.listOutbox()).toHaveLength(0);
    expect(await db.getMeta('last_server_time')).toBe('2026-01-01T12:00:00.000Z');
  });

  it('sends since = last server_time - 5s', async () => {
    await db.setMeta('last_server_time', '2026-01-01T12:00:00.000Z');
    mockRequest.mockResolvedValue(empty());
    await syncNow();
    expect(mockRequest.mock.calls[0][2].since).toBe('2026-01-01T11:59:55.000Z');
  });

  it('sends since = null on first sync', async () => {
    mockRequest.mockResolvedValue(empty());
    await syncNow();
    expect(mockRequest.mock.calls[0][2].since).toBeNull();
  });

  it('removes applied/ignored items but keeps an item edited again mid-flight', async () => {
    await saveSleep(sleepIn(ID(1), 1));
    mockRequest.mockImplementationOnce(async () => {
      await saveSleep(sleepIn(ID(1), 4)); // edited while the request is in flight
      return empty();
    });
    mockRequest.mockResolvedValue(empty());
    await syncNow();
    // the newer edit was pushed in a follow-up round, then removed
    expect(mockRequest).toHaveBeenCalledTimes(2);
    expect(mockRequest.mock.calls[1][2].changes.sleep[0].quality).toBe(4);
    expect(await db.listOutbox()).toHaveLength(0);
  });

  it('delete tombstones are pushed as deleted:true', async () => {
    await saveSleep(sleepIn(ID(1)));
    await deleteSleep(ID(1));
    mockRequest.mockResolvedValue(empty());
    await syncNow();
    expect(mockRequest.mock.calls[0][2].changes.sleep).toEqual([
      expect.objectContaining({ id: ID(1), deleted: true }),
    ]);
  });
});

describe('rejected', () => {
  it('marks rejected items failed, surfaces the count and never resends them', async () => {
    await saveSleep(sleepIn(ID(1)));
    await saveWeight({ id: ID(2), kg: 80 });
    mockRequest.mockResolvedValueOnce(
      empty({ rejected: [{ table: 'sleep', id: ID(1), reason: 'bad' }] }),
    );
    const r = await syncNow();
    expect(r.failed).toBe(1);
    expect(getFailedCount()).toBe(1);
    const out = await db.listOutbox();
    expect(out).toHaveLength(1);
    expect(out[0]).toMatchObject({ id: ID(1), failed: true, reason: 'bad' });

    mockRequest.mockResolvedValue(empty());
    await syncNow();
    expect(mockRequest).toHaveBeenCalledTimes(2);
    expect(mockRequest.mock.calls[1][2].changes.sleep).toEqual([]);
  });

  it('a new edit of a failed item gets queued again', async () => {
    await saveSleep(sleepIn(ID(1)));
    mockRequest.mockResolvedValueOnce(empty({ rejected: [{ table: 'sleep', id: ID(1), reason: 'bad' }] }));
    await syncNow();
    await saveSleep(sleepIn(ID(1), 5));
    expect((await db.getOutboxItem('sleep', ID(1)))?.failed).toBe(false);
  });
});

describe('pull (last write wins)', () => {
  const pulled = (id: string, updated_at: string, kg: number, deleted = false) => ({
    id,
    updated_at,
    deleted,
    synced_at: updated_at,
    measured_at: T0,
    kg,
  });

  it('applies unknown rows and newer rows, ignores older and equal ones', async () => {
    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T09:00:00.000Z', 70)] } }),
    );
    await syncNow();
    expect(await db.getRow('weight', ID(1))).toMatchObject({ data: { kg: 70 } });
    expect((await db.getRow('weight', ID(1)))?.data.synced_at).toBeUndefined();

    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T08:00:00.000Z', 60)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.data.kg).toBe(70);

    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T09:00:00.000Z', 61)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.data.kg).toBe(70);

    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T09:30:00.000Z', 72)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.data.kg).toBe(72);
  });

  it('applies a pulled tombstone', async () => {
    await db.putRow('weight', { id: ID(1), updated_at: T0, deleted: false, data: { kg: 70 } });
    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T11:00:00.000Z', 70, true)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.deleted).toBe(true);
  });

  it('preserves a pending local change that is newer than the incoming row', async () => {
    await saveWeight({ id: ID(1), kg: 99 }); // stamped at fake "now" 12:00
    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T11:00:00.000Z', 50)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.data.kg).toBe(99);
  });

  it('applies an incoming row newer than a pending local change', async () => {
    await saveWeight({ id: ID(1), kg: 99 });
    mockRequest.mockResolvedValueOnce(
      empty({ changes: { weight: [pulled(ID(1), '2026-01-01T13:00:00.000Z', 50)] } }),
    );
    await syncNow();
    expect((await db.getRow('weight', ID(1)))?.data.kg).toBe(50);
  });

  it('pages while has_more using since = server_time (no overlap) and stores the final server_time', async () => {
    await db.setMeta('last_server_time', '2026-01-01T11:00:00.000Z');
    mockRequest
      .mockResolvedValueOnce(
        empty({
          has_more: true,
          server_time: '2026-01-01T11:30:00.000Z',
          changes: { weight: [pulled(ID(1), '2026-01-01T11:10:00.000Z', 1)] },
        }),
      )
      .mockResolvedValueOnce(
        empty({
          has_more: false,
          server_time: '2026-01-01T12:00:00.000Z',
          changes: { weight: [pulled(ID(2), '2026-01-01T11:40:00.000Z', 2)] },
        }),
      );
    const r = await syncNow();
    expect(mockRequest).toHaveBeenCalledTimes(2);
    expect(mockRequest.mock.calls[0][2].since).toBe('2026-01-01T10:59:55.000Z');
    expect(mockRequest.mock.calls[1][2].since).toBe('2026-01-01T11:30:00.000Z');
    expect(r.pulled).toBe(2);
    expect(await db.getMeta('last_server_time')).toBe('2026-01-01T12:00:00.000Z');
  });

  it('does not store server_time if paging fails midway', async () => {
    mockRequest
      .mockResolvedValueOnce(empty({ has_more: true, server_time: '2026-01-01T11:30:00.000Z' }))
      .mockRejectedValueOnce(new Error('boom'));
    await syncNow();
    expect(await db.getMeta('last_server_time')).toBeNull();
  });
});

describe('engine control', () => {
  it('is single-flight: concurrent calls share one request', async () => {
    let release: (v: SyncResponse) => void = () => undefined;
    mockRequest.mockImplementationOnce(
      () =>
        new Promise<SyncResponse>((res) => {
          release = res;
        }),
    );
    mockRequest.mockResolvedValue(empty());
    const a = syncNow();
    const b = syncNow();
    expect(a).toBe(b);
    await jest.advanceTimersByTimeAsync(0);
    expect(mockRequest).toHaveBeenCalledTimes(1);
    release(empty());
    await a;
    // the joined call asked for one more pass
    expect(mockRequest).toHaveBeenCalledTimes(2);
  });

  it('backoffDelay doubles and caps', () => {
    expect(backoffDelay(0)).toBe(0);
    expect(backoffDelay(1)).toBe(1000);
    expect(backoffDelay(2)).toBe(2000);
    expect(backoffDelay(4)).toBe(8000);
    expect(backoffDelay(30)).toBe(5 * 60_000);
  });

  it('backs off after a failure: plain calls are skipped, the timer retries, force bypasses', async () => {
    mockRequest.mockRejectedValueOnce(new Error('down'));
    const r1 = await syncNow();
    expect(r1.ok).toBe(false);
    expect(mockRequest).toHaveBeenCalledTimes(1);

    const skipped = await syncNow();
    expect(skipped.skipped).toBe(true);
    expect(mockRequest).toHaveBeenCalledTimes(1);

    mockRequest.mockRejectedValueOnce(new Error('still down'));
    await jest.advanceTimersByTimeAsync(1000); // first backoff elapsed -> retry
    expect(mockRequest).toHaveBeenCalledTimes(2);

    // second backoff is 2 s: not yet at 1 s
    await jest.advanceTimersByTimeAsync(1000);
    expect(mockRequest).toHaveBeenCalledTimes(2);

    mockRequest.mockResolvedValue(empty());
    const forced = await syncNow({ force: true });
    expect(forced.ok).toBe(true);
    expect(mockRequest).toHaveBeenCalledTimes(3);
    // success resets the backoff
    await syncNow();
    expect(mockRequest).toHaveBeenCalledTimes(4);
  });

  it('runs a periodic sync every 60 s while started', async () => {
    mockRequest.mockResolvedValue(empty());
    startEngine();
    await jest.advanceTimersByTimeAsync(0);
    const initial = mockRequest.mock.calls.length;
    await jest.advanceTimersByTimeAsync(60_000);
    expect(mockRequest.mock.calls.length).toBe(initial + 1);
  });

  it('save triggers a sync once the engine is started', async () => {
    mockRequest.mockResolvedValue(empty());
    startEngine();
    await jest.advanceTimersByTimeAsync(0);
    mockRequest.mockClear();
    await saveWeight({ id: ID(5), kg: 70 });
    await jest.advanceTimersByTimeAsync(0);
    expect(mockRequest).toHaveBeenCalledTimes(1);
    expect(await db.listOutbox()).toHaveLength(0);
  });
});
