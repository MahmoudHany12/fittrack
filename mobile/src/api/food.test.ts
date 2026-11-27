import {
  addFavorite,
  clampServings,
  createFoodLog,
  deleteFoodLog,
  getFoodSummary,
  isSearchable,
  newId,
  previewTotals,
  removeFavorite,
  searchFoods,
  stepServings,
  todayUtc,
} from './food';

const mockAuthedRequest = jest.fn();
jest.mock('./client', () => ({
  authedRequest: (...args: unknown[]) => mockAuthedRequest(...args),
}));

beforeEach(() => {
  mockAuthedRequest.mockReset();
  mockAuthedRequest.mockResolvedValue(undefined);
});

describe('request shapes', () => {
  it('searches with a trimmed, encoded query', async () => {
    await searchFoods('  oat & milk ');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/foods/search?q=oat%20%26%20milk');
  });

  it('creates a log with a generated id when none is given', async () => {
    await createFoodLog({ food_id: 'f1', servings: 1.5 });
    const [path, method, body] = mockAuthedRequest.mock.calls[0];
    expect(path).toBe('/food-logs');
    expect(method).toBe('POST');
    expect(body.food_id).toBe('f1');
    expect(body.servings).toBe(1.5);
    expect(body.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  });

  it('keeps a supplied id so retries are idempotent', async () => {
    await createFoodLog({ id: 'fixed-id', food_id: 'f1', servings: 1 });
    expect(mockAuthedRequest.mock.calls[0][2].id).toBe('fixed-id');
  });

  it('uses PUT/DELETE for favorites and DELETE for logs', async () => {
    await addFavorite('a b');
    await removeFavorite('x');
    await deleteFoodLog('l1');
    expect(mockAuthedRequest.mock.calls).toEqual([
      ['/favorites/a%20b', 'PUT'],
      ['/favorites/x', 'DELETE'],
      ['/food-logs/l1', 'DELETE'],
    ]);
  });

  it('asks for the summary of a date', async () => {
    await getFoodSummary('2026-11-27');
    expect(mockAuthedRequest).toHaveBeenCalledWith('/food-logs/summary?date=2026-11-27');
  });
});

describe('helpers', () => {
  it('generates distinct ids', () => {
    expect(newId()).not.toBe(newId());
  });

  it('only searches with 2+ characters', () => {
    expect(isSearchable('a')).toBe(false);
    expect(isSearchable(' a ')).toBe(false);
    expect(isSearchable('ab')).toBe(true);
  });

  it('clamps and steps servings in half steps', () => {
    expect(clampServings(0)).toBe(0.5);
    expect(clampServings(100)).toBe(20);
    expect(clampServings(1.2)).toBe(1);
    expect(clampServings(Number.NaN)).toBe(1);
    expect(stepServings(1, 1)).toBe(1.5);
    expect(stepServings(0.5, -1)).toBe(0.5);
    expect(stepServings(20, 1)).toBe(20);
  });

  it('previews totals like the server', () => {
    const food = { kcal: 250, protein_g: 10, carbs_g: 20, fat_g: 5 };
    expect(previewTotals(food, 1.5)).toEqual({ kcal: 375, protein_g: 15, carbs_g: 30, fat_g: 7.5 });
  });

  it('formats today as a UTC date', () => {
    expect(todayUtc(new Date('2026-11-27T23:59:00Z'))).toBe('2026-11-27');
  });
});
