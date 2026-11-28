import { authedRequest } from './client';

export type WindowState = 'open' | 'recovering' | 'needs_rest' | 'missed';

export type TrackLevel = { level: number; points?: number; progress: number; value: number };

/** Strength and cardio carry a workout window on top of the level. */
export type WindowTrack = TrackLevel & {
  state: WindowState;
  opens_at: string | null;
  closes_at: string | null;
  /** open: remaining share of the window (1 -> 0). recovering: progress to opening (0 -> 1). */
  fraction: number;
  protein_bonus?: boolean;
};

export type PersonalBest = {
  exercise_id?: string;
  exercise?: string;
  kind?: 'weight' | 'one_rep_max' | string;
  weight_kg?: number;
  reps?: number;
  one_rep_max_kg?: number;
};

/** Contract: docs/progression.md "API summary". */
export type Progression = {
  tracks: { strength: WindowTrack; cardio: WindowTrack; focus: TrackLevel };
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

/** "Bench press: 82.5 kg" style line for a new personal best. */
export function bestLine(b: PersonalBest): string {
  const name = b.exercise ?? 'Exercise';
  if (typeof b.one_rep_max_kg === 'number') return `${name}: est. 1RM ${round1(b.one_rep_max_kg)} kg`;
  if (typeof b.weight_kg === 'number') return `${name}: ${round1(b.weight_kg)} kg`;
  return name;
}
