import { authedRequest } from './client';

export type ProgramExercise = {
  exercise_id: string;
  name: string;
  sets: number;
  reps: number;
  /** null when there is no history yet: "pick a starting weight". */
  target_weight_kg: number | null;
  rest_s?: number | null;
  superset_with?: string | null;
};

export type ProgramSession = {
  program?: string;
  week?: number;
  day?: number;
  title?: string;
  exercises: ProgramExercise[];
};

export const programKeys = { today: ['program'] as const };

/** Accepts the session directly or wrapped as { session }. Rest day / empty -> null. */
export function getProgram(): Promise<ProgramSession | null> {
  return authedRequest<(ProgramSession & { session?: ProgramSession | null }) | null>('/program').then(
    (r) => {
      const s = r && 'session' in r && r.session !== undefined ? r.session : r;
      return s && Array.isArray(s.exercises) && s.exercises.length > 0 ? s : null;
    },
  );
}

export function restText(s: number | null | undefined): string {
  if (typeof s !== 'number' || !(s > 0)) return '';
  return s >= 60 && s % 60 === 0 ? `Rest ${s / 60} min` : `Rest ${Math.round(s)} s`;
}

export function targetText(kg: number | null | undefined): string {
  return typeof kg === 'number' && Number.isFinite(kg)
    ? `Target ${Math.round(kg * 10) / 10} kg`
    : 'Pick a starting weight';
}

export type ProgramPrefill = { id: string; sets: number; reps: number; kg: number };

/** Params for /add/workout: `program` is a JSON list of { id, sets, reps, kg }. */
export function startParams(s: ProgramSession): { program: string } {
  const list: ProgramPrefill[] = s.exercises.map((e) => ({
    id: e.exercise_id,
    sets: e.sets,
    reps: e.reps,
    kg: e.target_weight_kg ?? 0,
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
        typeof p.id === 'string' &&
        Number.isFinite(p.sets) &&
        Number.isFinite(p.reps) &&
        Number.isFinite(p.kg),
    );
  } catch {
    return [];
  }
}
