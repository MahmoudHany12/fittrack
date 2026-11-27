import type { HealthCategory } from '@/api/health';

export type { HealthCategory };

/** Raw step record from the platform (any granularity; aggregated per day later). */
export type StepRecord = { start: Date; count: number };

/** A workout as reported by the platform. `id` is the platform's stable record id. */
export type WorkoutRecord = { id: string; start: Date; end: Date; kind: string };

/**
 * A sleep segment. `asleep` is false for "in bed" / "awake" stages, which are ignored.
 * Providers that report whole sessions return one segment per session.
 */
export type SleepSegment = { id: string; start: Date; end: Date; asleep: boolean };

export interface HealthProvider {
  /** Whether the platform health store can be used (false in Expo Go, web, simulators without it). */
  readonly available: boolean;
  /** Ask the OS for read access. Resolves true when the request was handled (not necessarily granted on iOS). */
  requestPermissions(categories: readonly HealthCategory[]): Promise<boolean>;
  readSteps(since: Date): Promise<StepRecord[]>;
  readWorkouts(since: Date): Promise<WorkoutRecord[]>;
  readSleep(since: Date): Promise<SleepSegment[]>;
}
