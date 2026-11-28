import { authedRequest } from './client';

export type ProgramExercise = {
  /** null when the exercise is not in the catalog. */
  exercise_id: string | null;
  name: string;
  sets: number;
  reps: number;
  /** null when there is no history yet: "pick a starting weight". */
  weight_kg: number | null;
  reason: 'start' | 'increase' | 'repeat' | 'deload';
  note: string;
  /** "A" marks a superset pair; null otherwise. */
  superset: string | null;
};

export type ProgramSession = {
  name: string;
  title: string;
  week: number;
  day: number;
  sets: number;
  completed: boolean;
  is_a: boolean;
  weeks: number;
  days_per_week: number;
  sessions_done: number;
  rest: { min_s: number; max_s: number };
  exercises: ProgramExercise[];
};

export const programKeys = { today: ['program'] as const };

export function getProgram(): Promise<ProgramSession> {
  return authedRequest<ProgramSession>('/program');
}

export function restText(rest: { min_s: number; max_s: number } | null | undefined): string {
  if (!rest || !(rest.max_s > 0)) return '';
  const f = (s: number) => (s >= 60 && s % 60 === 0 ? `${s / 60} min` : `${s} s`);
  return rest.min_s === rest.max_s ? `Rest ${f(rest.max_s)}` : `Rest ${f(rest.min_s)} to ${f(rest.max_s)}`;
}

export function targetText(kg: number | null | undefined): string {
  return typeof kg === 'number' && Number.isFinite(kg)
    ? `Target ${Math.round(kg * 10) / 10} kg`
    : 'Pick a starting weight';
}

export type ProgramGroup = { superset: string | null; items: ProgramExercise[] };

/** Consecutive exercises sharing a superset letter are shown together. */
export function groupExercises(list: ProgramExercise[]): ProgramGroup[] {
  const groups: ProgramGroup[] = [];
  for (const e of list) {
    const last = groups[groups.length - 1];
    if (e.superset && last && last.superset === e.superset) last.items.push(e);
    else groups.push({ superset: e.superset, items: [e] });
  }
  return groups;
}

/** `id` is null when only the name is known; the workout form then looks it up by name. */
export type ProgramPrefill = { id: string | null; name: string; sets: number; reps: number; kg: number };

/** Params for /add/workout: `program` is a JSON list of prefill entries. */
export function startParams(s: ProgramSession): { program: string } {
  const list: ProgramPrefill[] = s.exercises.map((e) => ({
    id: e.exercise_id,
    name: e.name,
    sets: e.sets,
    reps: e.reps,
    kg: e.weight_kg ?? 0,
  }));
  return { program: JSON.stringify(list) };
}

export function parsePrefill(raw: string | string[] | undefined): ProgramPrefill[] {
  const str = Array.isArray(raw) ? raw[0] : raw;
  if (!str) return [];
  try {
    const v: unknown = JSON.parse(str);
    if (!Array.isArray(v)) return [];
    return v.filter(
      (p): p is ProgramPrefill =>
        !!p &&
        (p.id === null || typeof p.id === 'string') &&
        typeof p.name === 'string' &&
        Number.isFinite(p.sets) &&
        Number.isFinite(p.reps) &&
        Number.isFinite(p.kg),
    );
  } catch {
    return [];
  }
}
