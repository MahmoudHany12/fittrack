import { getProgram, parsePrefill, restText, startParams, targetText } from './program';
import { PROTEIN_TIP, REST_MESSAGE, bestLine, getProgression } from './progression';
import { dateText, getRecords, kgText } from './records';

const mockAuthedRequest = jest.fn();
jest.mock('./client', () => {
  const actual = jest.requireActual('./client');
  return { ...actual, authedRequest: (...args: unknown[]) => mockAuthedRequest(...args) };
});

beforeEach(() => mockAuthedRequest.mockReset());

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

  it('bestLine prefers 1RM, then weight', () => {
    expect(bestLine({ exercise: 'Squat', one_rep_max_kg: 112.46 })).toBe('Squat: est. 1RM 112.5 kg');
    expect(bestLine({ exercise: 'Squat', weight_kg: 100 })).toBe('Squat: 100 kg');
    expect(bestLine({})).toBe('Exercise');
  });
});

describe('records api', () => {
  it('gets /records, accepting a list or a wrapped list', async () => {
    const r = [{ exercise_id: 'e', exercise: 'Squat', heaviest_kg: 100, best_1rm_kg: 110, achieved_at: null }];
    mockAuthedRequest.mockResolvedValueOnce(r);
    expect(await getRecords()).toEqual(r);
    mockAuthedRequest.mockResolvedValueOnce({ records: r });
    expect(await getRecords()).toEqual(r);
    expect(mockAuthedRequest).toHaveBeenCalledWith('/records');
  });

  it('formats kg and dates', () => {
    expect(kgText(82.54)).toBe('82.5 kg');
    expect(kgText(null)).toBe('-');
    expect(dateText(null)).toBe('');
    expect(dateText('nope')).toBe('');
    expect(dateText('2026-06-15T10:00:00Z')).not.toBe('');
  });
});

describe('program api', () => {
  const session = {
    exercises: [
      { exercise_id: 'sq', name: 'Back squat', sets: 3, reps: 5, target_weight_kg: 80, rest_s: 120 },
      { exercise_id: 'bp', name: 'Bench press', sets: 3, reps: 5, target_weight_kg: null },
    ],
  };

  it('gets /program and unwraps a session; empty becomes null', async () => {
    mockAuthedRequest.mockResolvedValueOnce(session);
    expect(await getProgram()).toEqual(session);
    mockAuthedRequest.mockResolvedValueOnce({ session });
    expect(await getProgram()).toEqual(session);
    mockAuthedRequest.mockResolvedValueOnce({ session: null });
    expect(await getProgram()).toBeNull();
    mockAuthedRequest.mockResolvedValueOnce({ exercises: [] });
    expect(await getProgram()).toBeNull();
    expect(mockAuthedRequest).toHaveBeenCalledWith('/program');
  });

  it('texts', () => {
    expect(restText(120)).toBe('Rest 2 min');
    expect(restText(90)).toBe('Rest 90 s');
    expect(restText(null)).toBe('');
    expect(targetText(82.5)).toBe('Target 82.5 kg');
    expect(targetText(null)).toBe('Pick a starting weight');
  });

  it('startParams round-trips through parsePrefill', () => {
    const { program } = startParams(session);
    expect(parsePrefill(program)).toEqual([
      { id: 'sq', sets: 3, reps: 5, kg: 80 },
      { id: 'bp', sets: 3, reps: 5, kg: 0 },
    ]);
  });

  it('parsePrefill ignores bad input', () => {
    expect(parsePrefill(undefined)).toEqual([]);
    expect(parsePrefill('not json')).toEqual([]);
    expect(parsePrefill('{"a":1}')).toEqual([]);
    expect(parsePrefill(['[{"id":"x","sets":1,"reps":2,"kg":3},{"id":1}]'])).toEqual([
      { id: 'x', sets: 1, reps: 2, kg: 3 },
    ]);
  });
});
