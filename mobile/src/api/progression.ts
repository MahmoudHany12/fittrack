import { authedRequest } from './client';

export type WindowState = 'open' | 'recovering' | 'needs_rest' | 'missed';

export type LevelInfo = { level: number; points?: number; progress: number; value: number };

/** Focus only has level info (fields flattened into the track). */
export type FocusTrack = LevelInfo;

/** Strength and cardio carry a workout window; level info fields are flattened into the track. */
export type WindowTrack = LevelInfo & {
  state: WindowState;
  opens_at: string | null;
  closes_at: string | null;
  /** open: remaining share of the window (1 -> 0). recovering: progress to opening (0 -> 1). */
  fraction: number;
  protein_bonus?: boolean;
};

export type PersonalBest = {
  exercise_id: string;
  exercise_name: string;
  kind: 'weight' | 'e1rm';
  value: number;
  previous: number | null;
  weight_kg: number;
  reps: number;
  workout_id?: string;
  achieved_at?: string;
};

/** Contract: docs/progression.md "API summary". */
export type Progression = {
  tracks: { strength: WindowTrack; cardio: WindowTrack; focus: FocusTrack };
  ring: { minutes: number; goal: number; closed: boolean };
  hints: string[];
  new_personal_bests: PersonalBest[];
};

export function getProgression(): Promise<Progression> {
  return authedRequest<Progression>('/progression');
}

/** Shown after a workout is saved (docs/progression.md "Daily ring and post-workout"). */
export const REST_MESSAGE = 'Workout complete — time to rest and recover.';
export const PROTEIN_TIP = 'Have ~25 g protein in the next 2 hours to recover twice as fast.';

const round1 = (n: number) => Math.round(n * 10) / 10;

/** "Bench Press: 82.5 kg (was 80)" for weight, "Bench Press: est. 1RM 95 kg" for e1rm. */
export function bestLine(b: PersonalBest): string {
  if (b.kind === 'e1rm') return `${b.exercise_name}: est. 1RM ${round1(b.value)} kg`;
  const was = b.previous != null ? ` (was ${round1(b.previous)})` : '';
  return `${b.exercise_name}: ${round1(b.value)} kg${was}`;
}
