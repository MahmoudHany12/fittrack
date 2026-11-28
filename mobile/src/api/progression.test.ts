import {
  getProgram,
  groupExercises,
  parsePrefill,
  restText,
  startParams,
  targetText,
  type ProgramExercise,
  type ProgramSession,
} from './program';
import { PROTEIN_TIP, REST_MESSAGE, bestLine, getProgression, type PersonalBest } from './progression';
import { dateText, getRecords, kgText, liftText } from './records';

const mockAuthedRequest = jest.fn();
jest.mock('./client', () => {
  const actual = jest.requireActual('./client');
  return { ...actual, authedRequest: (...args: unknown[]) => mockAuthedRequest(...args) };
});

beforeEach(() => mockAuthedRequest.mockReset());

const pb = (p: Partial<PersonalBest>): PersonalBest => ({
  exercise_id: 'e',
  exercise_name: 'Bench Press',
  kind: 'weight',
  value: 82.5,
  previous: 80,
  weight_kg: 82.5,
  reps: 5,
  ...p,
});

describe('progression api', () => {
  it('gets /progression', async () => {
    mockAuthedRequest.mockResolvedValue({ hints: [] });
    await getProgression();
    expect(mockAuthedRequest).toHaveBeenCalledWith('/progression');
  });

  it('has the post-workout copy', () => {
    expect(REST_MESSAGE).toBe('Workout complete — time to rest and recover.');
    expect(PROTEIN_TIP).toContain('25 g protein');
  });

  it('bestLine formats weight and e1rm bests', () => {
    expect(bestLine(pb({}))).toBe('Bench Press: 82.5 kg (was 80)');
    expect(bestLine(pb({ previous: null }))).toBe('Bench Press: 82.5 kg');
    expect(bestLine(pb({ kind: 'e1rm', value: 95 }))).toBe('Bench Press: est. 1RM 95 kg');
  });
});

describe('records api', () => {
  it('gets /records as a bare array', async () => {
    const r = [{ exercise_id: 'e', exercise_name: 'Squat' }];
    mockAuthedRequest.mockResolvedValueOnce(r);
    expect(await getRecords()).toEqual(r);
    expect(mockAuthedRequest).toHaveBeenCalledWith('/records');
  });

  it('formats kg, lifts and dates', () => {
    expect(kgText(82.54)).toBe('82.5 kg');
    expect(kgText(null)).toBe('-');
    expect(liftText(100, 5)).toBe('100 kg × 5');
    expect(liftText(100, null)).toBe('100 kg');
    expect(dateText(null)).toBe('');
    expect(dateText('nope')).toBe('');
    expect(dateText('2026-06-15T10:00:00Z')).not.toBe('');
  });
});

const ex = (p: Partial<ProgramExercise>): ProgramExercise => ({
  exercise_id: 'x',
  name: 'X',
  sets: 3,
  reps: 5,
  weight_kg: 80,
  reason: 'repeat',
  note: '',
  superset: null,
  ...p,
});

const session: ProgramSession = {
  name: 'Strength basics',
  title: 'Day A',
  week: 1,
  day: 1,
  sets: 3,
  completed: false,
  is_a: true,
  weeks: 4,
  days_per_week: 3,
  sessions_done: 0,
  rest: { min_s: 60, max_s: 120 },
  exercises: [
    ex({ exercise_id: 'sq', name: 'Back squat', superset: 'A' }),
    ex({ exercise_id: null, name: 'Kettlebell swing', reps: 15, weight_kg: null, superset: 'A' }),
    ex({ exercise_id: 'bp', name: 'Bench press' }),
  ],
};

describe('program api', () => {
  it('gets /program', async () => {
    mockAuthedRequest.mockResolvedValueOnce(session);
    expect(await getProgram()).toEqual(session);
    expect(mockAuthedRequest).toHaveBeenCalledWith('/program');
  });

  it('texts', () => {
    expect(restText({ min_s: 60, max_s: 120 })).toBe('Rest 1 min to 2 min');
    expect(restText({ min_s: 90, max_s: 90 })).toBe('Rest 90 s');
    expect(restText(null)).toBe('');
    expect(targetText(82.5)).toBe('Target 82.5 kg');
    expect(targetText(null)).toBe('Pick a starting weight');
  });

  it('groups superset pairs together', () => {
    const g = groupExercises(session.exercises);
    expect(g.map((x) => [x.superset, x.items.length])).toEqual([
      ['A', 2],
      [null, 1],
    ]);
  });

  it('startParams round-trips through parsePrefill, keeping null ids', () => {
    expect(parsePrefill(startParams(session).program)).toEqual([
      { id: 'sq', name: 'Back squat', sets: 3, reps: 5, kg: 80 },
      { id: null, name: 'Kettlebell swing', sets: 3, reps: 15, kg: 0 },
      { id: 'bp', name: 'Bench press', sets: 3, reps: 5, kg: 80 },
    ]);
  });

  it('parsePrefill ignores bad input', () => {
    expect(parsePrefill(undefined)).toEqual([]);
    expect(parsePrefill('not json')).toEqual([]);
    expect(parsePrefill('{"a":1}')).toEqual([]);
    expect(
      parsePrefill(['[{"id":"x","name":"X","sets":1,"reps":2,"kg":3},{"id":1}]']),
    ).toEqual([{ id: 'x', name: 'X', sets: 1, reps: 2, kg: 3 }]);
  });
});
