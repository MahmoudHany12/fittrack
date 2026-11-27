/* eslint-disable @typescript-eslint/no-require-imports */
import { dayWindows } from "./mapping";
import { noopProvider } from "./noop";
import type {
  HealthCategory,
  HealthProvider,
  SleepSegment,
  StepRecord,
  WorkoutRecord,
} from "./types";

const STEPS = "HKQuantityTypeIdentifierStepCount";
const SLEEP = "HKCategoryTypeIdentifierSleepAnalysis";
const WORKOUT = "HKWorkoutTypeIdentifier";
const LIMIT = 10000;

/** HKWorkoutActivityType raw values for the common kinds; everything else is "workout". */
const WORKOUT_KINDS: Record<number, string> = {
  13: "cycling",
  16: "elliptical",
  24: "hiking",
  35: "rowing",
  37: "running",
  46: "swimming",
  50: "strength training",
  52: "walking",
  57: "yoga",
  63: "hiit",
};

/** HKCategoryValueSleepAnalysis: 0 in bed, 2 awake; 1 unspecified, 3 core, 4 deep, 5 rem are asleep. */
export function isAsleepValue(v: number): boolean {
  return v === 1 || v === 3 || v === 4 || v === 5;
}

/** Lazy: importing the native module in Expo Go or jest would crash. */
function load(): any | null {
  try {
    return require("@kingstinct/react-native-healthkit");
  } catch {
    return null;
  }
}

const toDate = (v: any): Date => (v instanceof Date ? v : new Date(v));

/**
 * Daily step totals from a HealthKit statistics collection (cumulative sum per local day), which
 * merges sources the way the Health app does. One record per day, anchored at the day's start.
 */
export async function readDailySteps(
  hk: {
    queryStatisticsCollectionForQuantity: (...args: any[]) => Promise<any[]>;
  },
  since: Date,
  now: Date = new Date(),
): Promise<StepRecord[]> {
  const windows = dayWindows(since, now);
  if (windows.length === 0) return [];
  const rows = await hk.queryStatisticsCollectionForQuantity(
    STEPS,
    ["cumulativeSum"],
    windows[0].start,
    { day: 1 },
    {
      filter: { date: { startDate: windows[0].start, endDate: now } },
      unit: "count",
    },
  );
  const out: StepRecord[] = [];
  for (const r of rows ?? []) {
    const count = Number(r?.sumQuantity?.quantity ?? 0);
    if (r?.startDate && Number.isFinite(count) && count > 0) {
      out.push({ start: toDate(r.startDate), count });
    }
  }
  return out;
}

export function createIosProvider(): HealthProvider {
  const hk = load();
  if (!hk) return noopProvider;
  const range = (since: Date) => ({
    filter: { date: { startDate: since } },
    limit: LIMIT,
  });
  return {
    available: true,
    async requestPermissions(categories: readonly HealthCategory[]) {
      const toRead: string[] = [];
      if (categories.includes("steps")) toRead.push(STEPS);
      if (categories.includes("sleep")) toRead.push(SLEEP);
      if (categories.includes("workouts")) toRead.push(WORKOUT);
      try {
        await hk.requestAuthorization({ toRead });
        return true;
      } catch {
        return false;
      }
    },
    async readSteps(since: Date): Promise<StepRecord[]> {
      if (typeof hk.queryStatisticsCollectionForQuantity === "function") {
        return readDailySteps(hk, since);
      }
      // Older library without statistics collections: sum raw samples per day (mapSteps).
      const rows: any[] = await hk.queryQuantitySamples(STEPS, {
        ...range(since),
        unit: "count",
      });
      return rows.map((r) => ({
        start: toDate(r.startDate),
        count: Number(r.quantity),
      }));
    },
    async readWorkouts(since: Date): Promise<WorkoutRecord[]> {
      const rows: any[] = await hk.queryWorkoutSamples(range(since));
      return rows.map((r) => ({
        id: String(r.uuid),
        start: toDate(r.startDate),
        end: toDate(r.endDate),
        kind: WORKOUT_KINDS[Number(r.workoutActivityType)] ?? "workout",
      }));
    },
    async readSleep(since: Date): Promise<SleepSegment[]> {
      const rows: any[] = await hk.queryCategorySamples(SLEEP, range(since));
      return rows.map((r) => ({
        id: String(r.uuid),
        start: toDate(r.startDate),
        end: toDate(r.endDate),
        asleep: isAsleepValue(Number(r.value)),
      }));
    },
  };
}
