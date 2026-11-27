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

const PAGE_SIZE = 1000;
const MAX_PAGES = 50;

/** Health Connect ExerciseSessionRecord types for the common kinds; others are "workout". */
const EXERCISE_KINDS: Record<number, string> = {
  8: "cycling",
  9: "cycling",
  25: "elliptical",
  36: "hiit",
  37: "hiking",
  53: "rowing",
  54: "rowing",
  56: "running",
  57: "running",
  70: "strength training",
  74: "swimming",
  75: "swimming",
  79: "walking",
  83: "yoga",
};

const RECORD_TYPE: Record<HealthCategory, string> = {
  steps: "Steps",
  workouts: "ExerciseSession",
  sleep: "SleepSession",
};

/** Lazy: importing the native module in Expo Go or jest would crash. */
function load(): any | null {
  try {
    return require("react-native-health-connect");
  } catch {
    return null;
  }
}

/**
 * Daily step totals through Health Connect's aggregate API, which de-duplicates overlapping
 * records from several apps (phone and watch). One record per day, anchored at the day's start;
 * `mapSteps` turns each into the stable `steps-YYYY-MM-DD` sample.
 */
export async function readDailySteps(
  hc: { aggregateRecord: (req: any) => Promise<any> },
  since: Date,
  now: Date = new Date(),
): Promise<StepRecord[]> {
  const out: StepRecord[] = [];
  for (const w of dayWindows(since, now)) {
    const res = await hc.aggregateRecord({
      recordType: "Steps",
      timeRangeFilter: {
        operator: "between",
        startTime: w.start.toISOString(),
        endTime: w.end.toISOString(),
      },
    });
    const count = Number(res?.COUNT_TOTAL ?? 0);
    if (Number.isFinite(count) && count > 0)
      out.push({ start: w.start, count });
  }
  return out;
}

export function createAndroidProvider(): HealthProvider {
  const hc = load();
  if (!hc) return noopProvider;
  let ready: Promise<boolean> | null = null;
  const init = () =>
    (ready ??= Promise.resolve(hc.initialize()).catch(() => false));

  async function records(type: string, since: Date): Promise<any[]> {
    if (!(await init())) return [];
    const out: any[] = [];
    let pageToken: string | undefined;
    for (let page = 0; page < MAX_PAGES; page++) {
      const res: any = await hc.readRecords(type, {
        timeRangeFilter: {
          operator: "between",
          startTime: since.toISOString(),
          endTime: new Date().toISOString(),
        },
        pageSize: PAGE_SIZE,
        pageToken,
      });
      out.push(...(res.records ?? []));
      pageToken = res.pageToken;
      if (!pageToken) break;
    }
    return out;
  }

  return {
    available: true,
    async requestPermissions(categories: readonly HealthCategory[]) {
      if (!(await init())) return false;
      try {
        const granted: any[] = await hc.requestPermission(
          categories.map((c) => ({
            accessType: "read",
            recordType: RECORD_TYPE[c],
          })),
        );
        return categories.every((c) =>
          granted.some(
            (g) => g.recordType === RECORD_TYPE[c] && g.accessType === "read",
          ),
        );
      } catch {
        return false;
      }
    },
    async readSteps(since: Date): Promise<StepRecord[]> {
      if (!(await init())) return [];
      return readDailySteps(hc, since);
    },
    async readWorkouts(since: Date): Promise<WorkoutRecord[]> {
      return (await records("ExerciseSession", since)).map((r) => ({
        id: String(r.metadata?.id ?? ""),
        start: new Date(r.startTime),
        end: new Date(r.endTime),
        kind: EXERCISE_KINDS[Number(r.exerciseType)] ?? "workout",
      }));
    },
    async readSleep(since: Date): Promise<SleepSegment[]> {
      return (await records("SleepSession", since)).map((r) => ({
        id: String(r.metadata?.id ?? ""),
        start: new Date(r.startTime),
        end: new Date(r.endTime),
        asleep: true,
      }));
    },
  };
}
