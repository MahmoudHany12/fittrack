import { authedRequest } from './client';
import {
  dietSentence,
  formatRange,
  getWeeklySummary,
  isEmptyWeek,
  sleepSentence,
  trainingSentence,
  weightSentence,
  type WeeklySummary,
} from './summary';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

const empty: WeeklySummary = {
  from: '2026-03-09',
  to: '2026-03-15',
  training: { workouts: null, minutes: null, active_days: null, vs_prev_week_minutes: null },
  sleep: { avg_minutes: null, avg_quality: null, nights: null, vs_prev_week_minutes: null },
  weight: { start_kg: null, end_kg: null, change_kg: null, trend: null },
  diet: { avg_kcal: null, days_logged: null, target: null },
};

beforeEach(() => mockReq.mockReset());

describe('getWeeklySummary', () => {
  it('requests the default window', async () => {
    mockReq.mockResolvedValue(empty);
    await getWeeklySummary();
    expect(mockReq).toHaveBeenCalledWith('/summary/weekly');
  });
  it('passes week_start', async () => {
    mockReq.mockResolvedValue(empty);
    await getWeeklySummary('2026-03-09');
    expect(mockReq).toHaveBeenCalledWith('/summary/weekly?week_start=2026-03-09');
  });
});

describe('plain-language sentences', () => {
  it('training up, down, same and empty', () => {
    const t = { workouts: 3, minutes: 140, active_days: 3, vs_prev_week_minutes: 40 };
    expect(trainingSentence(t)).toBe('You trained 3 times, 140 min — up from last week');
    expect(trainingSentence({ ...t, vs_prev_week_minutes: -10 })).toContain('down from last week');
    expect(trainingSentence({ ...t, vs_prev_week_minutes: 0 })).toContain('the same as last week');
    expect(trainingSentence({ ...t, workouts: 1, vs_prev_week_minutes: null })).toBe(
      'You trained 1 time, 140 min',
    );
    expect(trainingSentence(empty.training)).toBe('No workouts logged this week.');
  });

  it('sleep', () => {
    expect(
      sleepSentence({ avg_minutes: 450, avg_quality: 4.5, nights: 2, vs_prev_week_minutes: -30 }),
    ).toBe('You slept 7 h 30 min a night on average (2 nights logged) — down 30 min from last week');
    expect(sleepSentence(empty.sleep)).toBe('No sleep logged this week.');
  });

  it('weight in metric and imperial', () => {
    const w = { start_kg: 80, end_kg: 78.5, change_kg: -1.5, trend: 'down' as const };
    expect(weightSentence(w)).toBe('Your weight went down 1.5 kg, from 80 kg to 78.5 kg.');
    expect(weightSentence(w, 'imperial')).toContain('lb');
    expect(weightSentence({ ...w, change_kg: 0.1, end_kg: 80.1, trend: 'flat' })).toBe(
      'Your weight stayed steady at 80.1 kg.',
    );
    expect(weightSentence(empty.weight)).toBe('No weigh-ins this week.');
  });

  it('diet', () => {
    expect(dietSentence({ avg_kcal: 2000, days_logged: 2, target: 2200 })).toBe(
      'You logged food on 2 days, about 2,000 kcal a day (target 2,200).',
    );
    expect(dietSentence({ avg_kcal: 1500, days_logged: 1, target: null })).toBe(
      'You logged food on 1 day, about 1,500 kcal a day.',
    );
    expect(dietSentence(empty.diet)).toBe('No food logged this week.');
  });

  it('detects an empty week and formats the range', () => {
    expect(isEmptyWeek(empty)).toBe(true);
    expect(isEmptyWeek({ ...empty, diet: { avg_kcal: 1, days_logged: 1, target: null } })).toBe(false);
    expect(formatRange('2026-03-09', '2026-03-15')).toBe('9 Mar – 15 Mar');
  });
});
