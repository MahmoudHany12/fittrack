import {
  clamp,
  createExercise,
  createWorkout,
  deleteWorkout,
  listWorkouts,
  newId,
  searchExercises,
  updateWorkout,
} from './workouts';
import { ApiError } from './client';

const mockAuthedRequest = jest.fn();
jest.mock('./client', () => {
  const actual = jest.requireActual('./client');
  return { ...actual, authedRequest: (...args: unknown[]) => mockAuthedRequest(...args) };
});

beforeEach(() => {
  mockAuthedRequest.mockReset();
  mockAuthedRequest.mockResolvedValue([]);
});

describe('searchExercises', () => {
  it('omits the query string when empty', async () => {
    await searchExercises('  ');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/exercises');
  });

  it('encodes the search text', async () => {
    await searchExercises('bench & fly');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/exercises?q=bench%20%26%20fly');
  });
});

describe('createExercise', () => {
  it('posts a trimmed name', async () => {
    await createExercise('  Sled Push ', 'legs');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/exercises', 'POST', {
      name: 'Sled Push',
      muscle_group: 'legs',
    });
  });
});

describe('workouts', () => {
  const input = {
    duration_s: 600,
    sets: [{ exercise_id: 'e1', reps: 8, weight_kg: 50 }],
  };

  it('createWorkout adds a client id when missing', async () => {
    await createWorkout(input);
    const [path, method, body] = mockAuthedRequest.mock.calls[0];
    expect(path).toBe('/workouts');
    expect(method).toBe('POST');
    expect(body.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(body.duration_s).toBe(600);
  });

  it('createWorkout keeps a supplied id so retries are idempotent', async () => {
    await createWorkout({ ...input, id: 'fixed-id' });
    expect(mockAuthedRequest.mock.calls[0][2].id).toBe('fixed-id');
  });

  it('listWorkouts builds the range query', async () => {
    await listWorkouts({ from: '2025-01-01T00:00:00Z', to: '2025-02-01T00:00:00Z' });
    expect(mockAuthedRequest).toHaveBeenCalledWith(
      '/workouts?from=2025-01-01T00%3A00%3A00Z&to=2025-02-01T00%3A00%3A00Z',
    );
    mockAuthedRequest.mockClear();
    await listWorkouts();
    expect(mockAuthedRequest).toHaveBeenCalledWith('/workouts');
  });

  it('updateWorkout and deleteWorkout use the id path', async () => {
    await updateWorkout('w1', input);
    expect(mockAuthedRequest).toHaveBeenCalledWith('/workouts/w1', 'PUT', input);
    await deleteWorkout('w1');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/workouts/w1', 'DELETE');
  });

  it('passes API errors through', async () => {
    mockAuthedRequest.mockRejectedValue(new ApiError(422, 'validation_failed', 'bad'));
    await expect(createWorkout(input)).rejects.toMatchObject({ status: 422 });
  });
});

describe('helpers', () => {
  it('newId produces unique v4 ids', () => {
    const ids = new Set(Array.from({ length: 50 }, () => newId()));
    expect(ids.size).toBe(50);
  });

  it('clamp bounds a number', () => {
    expect(clamp(-5, 0, 10)).toBe(0);
    expect(clamp(50, 0, 10)).toBe(10);
    expect(clamp(5, 0, 10)).toBe(5);
  });
});
