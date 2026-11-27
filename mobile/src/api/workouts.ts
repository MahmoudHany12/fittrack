import { authedRequest } from "./client";

export const MUSCLE_GROUPS = [
  "chest",
  "back",
  "legs",
  "shoulders",
  "arms",
  "core",
  "cardio",
] as const;
export type MuscleGroup = (typeof MUSCLE_GROUPS)[number];

export type Exercise = {
  id: string;
  name: string;
  muscle_group: MuscleGroup;
  custom: boolean;
};

export type WorkoutSetInput = {
  exercise_id: string;
  reps: number;
  weight_kg: number;
};
export type WorkoutSet = WorkoutSetInput & { id: string; position: number };

export type WorkoutInput = {
  /** Client generated id so a retried save never creates a duplicate. */
  id?: string;
  started_at?: string;
  duration_s: number;
  note?: string | null;
  /** Optional kind such as "running" (1-40 chars). Omitting it keeps the stored kind. */
  kind?: string | null;
  sets: WorkoutSetInput[];
};

export type Workout = {
  id: string;
  started_at: string;
  duration_s: number;
  note: string | null;
  kind: string | null;
  source: string;
  external_id: string | null;
  updated_at: string;
  sets: WorkoutSet[];
};

export const LIMITS = {
  duration_s: { min: 1, max: 86400 },
  reps: { min: 1, max: 1000 },
  weight_kg: { min: 0, max: 1000 },
  maxSets: 100,
} as const;

/** RFC 4122 v4 id, so the server can treat a retried create as the same workout. */
export function newId(): string {
  const bytes = new Uint8Array(16);
  for (let i = 0; i < 16; i++) bytes[i] = Math.floor(Math.random() * 256);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const h = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

export function clamp(n: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, n));
}

export function searchExercises(q = ""): Promise<Exercise[]> {
  const query = q.trim();
  const qs = query ? `?q=${encodeURIComponent(query)}` : "";
  return authedRequest<Exercise[]>(`/exercises${qs}`);
}

export function createExercise(
  name: string,
  muscleGroup: MuscleGroup,
): Promise<Exercise> {
  return authedRequest<Exercise>("/exercises", "POST", {
    name: name.trim(),
    muscle_group: muscleGroup,
  });
}

export function listWorkouts(
  range: { from?: string; to?: string } = {},
): Promise<Workout[]> {
  const params = new URLSearchParams();
  if (range.from) params.set("from", range.from);
  if (range.to) params.set("to", range.to);
  const qs = params.toString();
  return authedRequest<Workout[]>(`/workouts${qs ? `?${qs}` : ""}`);
}

export function createWorkout(input: WorkoutInput): Promise<Workout> {
  return authedRequest<Workout>("/workouts", "POST", {
    ...input,
    id: input.id ?? newId(),
  });
}

export function updateWorkout(
  id: string,
  input: WorkoutInput,
): Promise<Workout> {
  return authedRequest<Workout>(`/workouts/${id}`, "PUT", input);
}

export function deleteWorkout(id: string): Promise<void> {
  return authedRequest<void>(`/workouts/${id}`, "DELETE");
}
