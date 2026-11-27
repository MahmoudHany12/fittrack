import type { ImportBody, ImportResult } from "@/api/health";

import { readDailySteps as readAndroidSteps } from "./android";
import { readDailySteps as readIosSteps } from "./ios";
import {
  buildBatches,
  chunk,
  dayWindows,
  localDay,
  mapSleep,
  mapSteps,
  mapWorkouts,
} from "./mapping";
import { getHealthProvider } from "./provider";
import { runSync } from "./sync";
import type { HealthProvider } from "./types";

jest.mock("expo-secure-store", () => ({
  getItemAsync: jest.fn(async () => null),
  setItemAsync: jest.fn(async () => undefined),
  deleteItemAsync: jest.fn(async () => undefined),
}));
// Simulate Expo Go / a build without the native modules, whether or not they are installed.
jest.mock(
  "@kingstinct/react-native-healthkit",
  () => {
    throw new Error("native module unavailable");
  },
  { virtual: true },
);
jest.mock(
  "react-native-health-connect",
  () => {
    throw new Error("native module unavailable");
  },
  { virtual: true },
);
jest.mock("@/api/health", () => ({
  ...jest.requireActual("@/api/health"),
  importHealth: jest.fn(),
}));

const now = new Date(2026, 2, 10, 12, 0);
const at = (d: number, h: number, m = 0) => new Date(2026, 2, d, h, m);

describe("mapSteps", () => {
  it("sums records per local day with a stable id", () => {
    const items = mapSteps(
      [
        { start: at(9, 8), count: 1000 },
        { start: at(9, 18), count: 2500.4 },
        { start: at(10, 9), count: 300 },
      ],
      now,
    );
    expect(items).toEqual([
      { external_id: "steps-2026-03-09", date: "2026-03-09", count: 3500 },
      { external_id: "steps-2026-03-10", date: "2026-03-10", count: 300 },
    ]);
  });

  it("drops empty, invalid, future and absurd values", () => {
    expect(
      mapSteps(
        [
          { start: at(9, 8), count: 0 },
          { start: at(9, 8), count: Number.NaN },
          { start: at(11, 8), count: 50 },
          { start: at(8, 8), count: 2_000_000 },
        ],
        now,
      ),
    ).toEqual([]);
  });

  it("formats local days", () => {
    expect(localDay(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("mapWorkouts", () => {
  it("maps and de-duplicates by id, skipping invalid durations", () => {
    const items = mapWorkouts(
      [
        { id: "a", start: at(9, 7), end: at(9, 8), kind: " running " },
        { id: "a", start: at(9, 7), end: at(9, 8), kind: "running" },
        { id: "b", start: at(9, 7), end: at(9, 7), kind: "yoga" },
        { id: "c", start: at(9, 7), end: at(11, 7), kind: "walking" },
        { id: "d", start: at(9, 9), end: at(9, 10), kind: "" },
        { id: "", start: at(9, 9), end: at(9, 10), kind: "x" },
      ],
      now,
    );
    expect(items.map((i) => [i.external_id, i.duration_s, i.kind])).toEqual([
      ["workout-a", 3600, "running"],
      ["workout-d", 3600, "workout"],
    ]);
  });
});

describe("mapSleep", () => {
  it("merges close asleep segments into one night and ignores in-bed/awake", () => {
    const items = mapSleep(
      [
        {
          id: "inbed",
          start: at(9, 22, 30),
          end: at(10, 6, 30),
          asleep: false,
        },
        { id: "s2", start: at(10, 1), end: at(10, 3), asleep: true },
        { id: "s1", start: at(9, 23), end: at(10, 1), asleep: true },
        { id: "s3", start: at(10, 3, 20), end: at(10, 6), asleep: true },
      ],
      now,
    );
    expect(items).toEqual([
      {
        external_id: "sleep-s1",
        bedtime: at(9, 23).toISOString(),
        wake_time: at(10, 6).toISOString(),
      },
    ]);
  });

  it("splits sessions with a long gap and drops those over 24h", () => {
    const items = mapSleep(
      [
        { id: "night", start: at(8, 23), end: at(9, 6), asleep: true },
        { id: "nap", start: at(9, 14), end: at(9, 15), asleep: true },
        { id: "huge", start: at(5, 0), end: at(6, 12), asleep: true },
      ],
      now,
    );
    expect(items.map((i) => i.external_id)).toEqual([
      "sleep-night",
      "sleep-nap",
    ]);
  });
});

describe("batching", () => {
  it("chunks", () => {
    expect(chunk([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
    expect(chunk([], 2)).toEqual([]);
  });

  it("never exceeds the per-category size and zips chunks", () => {
    const steps = Array.from({ length: 5 }, (_, i) => ({
      external_id: `s${i}`,
      date: "2026-03-01",
      count: 1,
    }));
    const sleep = [{ external_id: "z", bedtime: "b", wake_time: "w" }];
    const batches = buildBatches({ steps, workouts: [], sleep }, 2);
    expect(batches).toHaveLength(3);
    expect(batches[0].steps).toHaveLength(2);
    expect(batches[0].sleep).toHaveLength(1);
    expect(batches[1].sleep).toBeUndefined();
    expect(batches[2].steps).toHaveLength(1);
    expect(batches[0].workouts).toBeUndefined();
  });

  it("caps the size at the server limit", () => {
    const steps = Array.from({ length: 2500 }, (_, i) => ({
      external_id: `s${i}`,
      date: "2026-03-01",
      count: 1,
    }));
    const batches = buildBatches({ steps, workouts: [], sleep: [] }, 5000);
    expect(batches.map((b) => b.steps?.length)).toEqual([1000, 1000, 500]);
  });

  it("sends nothing when there is nothing", () => {
    expect(buildBatches({ steps: [], workouts: [], sleep: [] })).toEqual([]);
  });
});

function fakeProvider(
  overrides: Partial<HealthProvider> = {},
): HealthProvider & {
  calls: Record<string, Date[]>;
} {
  const calls: Record<string, Date[]> = { steps: [], workouts: [], sleep: [] };
  return {
    available: true,
    calls,
    requestPermissions: async () => true,
    readSteps: async (since) => {
      calls.steps.push(since);
      return [{ start: at(9, 10), count: 4000 }];
    },
    readWorkouts: async (since) => {
      calls.workouts.push(since);
      return [{ id: "w1", start: at(9, 7), end: at(9, 8), kind: "running" }];
    },
    readSleep: async (since) => {
      calls.sleep.push(since);
      return [{ id: "z1", start: at(9, 0), end: at(9, 7), asleep: true }];
    },
    ...overrides,
  };
}

const okResult = (body: ImportBody): ImportResult => ({
  steps: { imported: body.steps?.length ?? 0, skipped: 0, duplicates: 0 },
  workouts: { imported: body.workouts?.length ?? 0, skipped: 0, duplicates: 0 },
  sleep: { imported: body.sleep?.length ?? 0, skipped: 0, duplicates: 0 },
});

describe("runSync", () => {
  it("reads only consented categories and totals the results", async () => {
    const provider = fakeProvider();
    const send = jest.fn(async (b: ImportBody) => okResult(b));
    const out = await runSync({
      provider,
      consents: { steps: true, workouts: false, sleep: true },
      lastSync: null,
      now,
      send,
    });
    expect(provider.calls.workouts).toHaveLength(0);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][0].workouts).toBeUndefined();
    expect(out.totals.steps.imported).toBe(1);
    expect(out.totals.sleep.imported).toBe(1);
    expect(out.batches).toBe(1);
  });

  it("first sync looks back 30 days; later syncs start at the last sync (whole days for steps)", async () => {
    const p1 = fakeProvider();
    const all = { steps: true, workouts: true, sleep: true };
    await runSync({
      provider: p1,
      consents: all,
      lastSync: null,
      now,
      send: async (b) => okResult(b),
    });
    expect(p1.calls.workouts[0]).toEqual(
      new Date(now.getTime() - 30 * 86400000),
    );

    const p2 = fakeProvider();
    const last = at(9, 15, 45);
    await runSync({
      provider: p2,
      consents: all,
      lastSync: last,
      now,
      send: async (b) => okResult(b),
    });
    expect(p2.calls.steps[0]).toEqual(at(9, 0));
    expect(p2.calls.workouts[0]).toEqual(last);
    expect(p2.calls.sleep[0].getTime()).toBeLessThan(last.getTime());
  });

  it("does not call the API when there is nothing to send", async () => {
    const send = jest.fn();
    const out = await runSync({
      provider: fakeProvider(),
      consents: { steps: false, workouts: false, sleep: false },
      lastSync: null,
      now,
      send,
    });
    expect(send).not.toHaveBeenCalled();
    expect(out.batches).toBe(0);
  });

  it("propagates a failed batch so the last-sync time is not advanced", async () => {
    const send = jest.fn().mockRejectedValue(new Error("offline"));
    await expect(
      runSync({
        provider: fakeProvider(),
        consents: { steps: true, workouts: true, sleep: true },
        lastSync: null,
        now,
        send,
      }),
    ).rejects.toThrow("offline");
  });
});

describe("getHealthProvider", () => {
  it("falls back to the no-op provider on web and without native modules", async () => {
    for (const os of ["web", "ios", "android"]) {
      const p = getHealthProvider(os);
      expect(p.available).toBe(false);
      expect(await p.requestPermissions(["steps"])).toBe(false);
      expect(await p.readSteps(now)).toEqual([]);
      expect(await p.readWorkouts(now)).toEqual([]);
      expect(await p.readSleep(now)).toEqual([]);
    }
  });
});

describe("dayWindows", () => {
  it("buckets by local day through today and clips the last window to now", () => {
    const w = dayWindows(at(8, 15), now);
    expect(w.map((x) => x.day)).toEqual([
      "2026-03-08",
      "2026-03-09",
      "2026-03-10",
    ]);
    expect(w[0].start).toEqual(at(8, 0));
    expect(w[0].end).toEqual(at(9, 0));
    expect(w[2].end).toEqual(now);
  });

  it("keeps only the most recent days when capped", () => {
    expect(dayWindows(at(1, 0), now, 2).map((x) => x.day)).toEqual([
      "2026-03-09",
      "2026-03-10",
    ]);
  });
});

describe("daily step totals", () => {
  it("android: one aggregate query per day, mapped to stable ids", async () => {
    const totals: Record<string, number> = {
      "2026-03-09": 8000,
      "2026-03-10": 0,
    };
    const aggregateRecord = jest.fn(async (req: any) => ({
      COUNT_TOTAL:
        totals[localDay(new Date(req.timeRangeFilter.startTime))] ?? 123,
    }));
    const records = await readAndroidSteps({ aggregateRecord }, at(9, 7), now);
    expect(aggregateRecord).toHaveBeenCalledTimes(2);
    expect(aggregateRecord.mock.calls[0][0].recordType).toBe("Steps");
    expect(mapSteps(records, now)).toEqual([
      { external_id: "steps-2026-03-09", date: "2026-03-09", count: 8000 },
    ]);
  });

  it("ios: maps statistics collection buckets to one record per day", async () => {
    const query = jest.fn(async (..._args: any[]) => [
      { startDate: at(9, 0), sumQuantity: { quantity: 4200, unit: "count" } },
      { startDate: at(10, 0), sumQuantity: { quantity: 900, unit: "count" } },
      { startDate: at(11, 0), sumQuantity: undefined },
    ]);
    const records = await readIosSteps(
      { queryStatisticsCollectionForQuantity: query },
      at(9, 7),
      now,
    );
    expect(query.mock.calls[0][1]).toEqual(["cumulativeSum"]);
    expect(query.mock.calls[0][3]).toEqual({ day: 1 });
    expect(mapSteps(records, now).map((s) => [s.external_id, s.count])).toEqual(
      [
        ["steps-2026-03-09", 4200],
        ["steps-2026-03-10", 900],
      ],
    );
  });
});
