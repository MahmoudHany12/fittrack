import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ScrollView } from 'react-native';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import type { Dashboard } from '@/api/dashboard';

import { HomeScreen } from './HomeScreen';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mockPush = jest.fn();
jest.mock('expo-router', () => ({ useRouter: () => ({ push: mockPush }) }));

const mockGet = jest.fn();
jest.mock('@/api/dashboard', () => ({ getDashboard: () => mockGet() }));

const mockHooks = {
  online: true,
  pending: 0,
  totals: { kcal: 0, minutes: 0 },
  weight: null as { kg: number; pending: boolean; at: number } | null,
};
jest.mock('@/sync/hooks', () => ({
  useOnline: () => mockHooks.online,
  usePendingCount: () => mockHooks.pending,
  usePendingTotals: () => mockHooks.totals,
  useLatestLocalWeight: () => mockHooks.weight,
}));

const full: Dashboard = {
  name: 'Alex',
  status: { key: 'well_rested', label: 'Well rested' },
  readiness: { score: 82 },
  sleep: { minutes: 460, quality: 4, label: 'Well rested' },
  diet: {
    kcal_eaten: 1420,
    kcal_target: 2200,
    label: 'On track',
    protein_g_eaten: 90,
    protein_target_g: 140,
  },
  exercise: { steps: 6200, workout_minutes_today: 30, workout_count_today: 1 },
  weight: { kg: 78.4, trend: 'down' },
  body: {
    weight_kg: 78.4,
    trend: 'down',
    body_fat_pct: 18.5,
    skeletal_muscle_kg: 34.2,
    measured_at: null,
  },
  workout_window: { minutes_today: 30, goal_minutes: 30, fraction: 1 },
  stats: {
    strength: { value: 40, level: 2, progress: 0.6 },
    cardio: { value: 60, level: 3, progress: 0.4 },
    focus: { value: 80, level: 4, progress: 0.2 },
  },
};

const empty: Dashboard = {
  name: 'Alex',
  status: { key: 'no_data', label: 'No data yet' },
  sleep: { minutes: null, quality: null, label: null },
  diet: { kcal_eaten: null, kcal_target: null, label: null },
  exercise: { steps: null, workout_minutes_today: null },
  weight: { kg: null, trend: null },
  stats: null,
};

async function flush() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

async function render(): Promise<ReactTestRenderer> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  let r!: ReactTestRenderer;
  await act(async () => {
    r = create(
      <QueryClientProvider client={client}>
        <HomeScreen />
      </QueryClientProvider>,
    );
  });
  await flush();
  return r;
}

type Json = { children?: (Json | string)[] | null };
function collect(n: Json | string | null | undefined): string[] {
  if (n == null) return [];
  if (typeof n === 'string') return [n];
  return (n.children ?? []).flatMap(collect);
}
const text = (r: ReactTestRenderer) => {
  const j = r.toJSON() as Json | Json[] | null;
  return (Array.isArray(j) ? j : [j]).flatMap(collect).join(' | ');
};
const byLabel = (r: ReactTestRenderer, prefix: string) =>
  r.root.findAll(
    (n) =>
      typeof n.props.accessibilityLabel === 'string' &&
      n.props.accessibilityLabel.startsWith(prefix) &&
      typeof n.props.onPress === 'function',
  )[0];

beforeEach(() => {
  mockPush.mockReset();
  mockGet.mockReset();
  mockHooks.online = true;
  mockHooks.pending = 0;
  mockHooks.weight = null;
  mockHooks.totals = { kcal: 0, minutes: 0 };
});

describe('HomeScreen', () => {
  it('shows a loading caption first', async () => {
    mockGet.mockReturnValue(new Promise(() => {}));
    const r = await render();
    expect(text(r)).toContain('Loading…');
  });

  it('renders header, stat cards and body rows', async () => {
    mockGet.mockResolvedValue(full);
    const r = await render();
    const t = text(r);
    expect(t).toContain('Alex');
    expect(t).toContain('Healthy');
    expect(t).toContain('| A |');
    expect(t).toContain('Fitness');
    expect(t).toContain('Strength Workout');
    expect(t).toContain('Cardio Workout');
    expect(t).toContain('Workout window');
    expect(t).toContain('Focus');
    expect(t).toContain('Level 2/4');
    expect(t).toContain('Level 4/4');
    expect(t).toContain('Body & Nutrition');
    expect(t).toContain('7 h 40 min');
    expect(t).toContain('1,420 / 2,200 kcal');
    expect(t).toContain('Protein 90 / 140 g');
    expect(t).toContain('78.4 kg');
    expect(t).toContain('Body fat 18.5% · Muscle 34.2 kg');
    expect(t).toContain('↘');
  });

  it('still renders three level-1 cards without stats or newer API fields', async () => {
    mockGet.mockResolvedValue({
      ...full,
      stats: null,
      body: undefined,
      workout_window: undefined,
      diet: { kcal_eaten: 1420, kcal_target: 2200, label: 'On track' },
    });
    const r = await render();
    const t = text(r);
    expect(t.match(/Level 1\/4/g)).toHaveLength(3);
    expect(t).toContain('Workout window');
    expect(t).not.toContain('Body fat');
    expect(t).not.toContain('Protein');
  });

  it('maps readiness to the status word', async () => {
    mockGet.mockResolvedValue({ ...full, readiness: { score: 50 } });
    expect(text(await render())).toContain('Okay');
    mockGet.mockResolvedValue({ ...full, readiness: { score: 20 } });
    expect(text(await render())).toContain('Tired');
  });

  it('shows the just-saved pending weight at once', async () => {
    mockGet.mockResolvedValue(full);
    mockHooks.weight = { kg: 80.2, pending: true, at: Date.now() };
    const r = await render();
    expect(text(r)).toContain('80.2 kg');
    expect(text(r)).not.toContain('78.4 kg');
  });

  it('shows friendly empty copy', async () => {
    mockGet.mockResolvedValue(empty);
    const r = await render();
    const t = text(r);
    expect(t).toContain('No sleep logged yet — tap to add');
    expect(t).toContain('No meals logged yet — tap to add');
    expect(t).toContain('No weight logged yet — tap to add');
  });

  it('navigates with one tap', async () => {
    mockGet.mockResolvedValue(full);
    const r = await render();
    await act(async () => byLabel(r, 'Sleep').props.onPress());
    await act(async () => byLabel(r, 'Diet').props.onPress());
    await act(async () => byLabel(r, 'Weight').props.onPress());
    await act(async () => byLabel(r, 'Strength Workout').props.onPress());
    expect(mockPush.mock.calls.map((c) => c[0])).toEqual([
      '/add/sleep',
      '/add/food',
      '/add/weight',
      '/add/workout',
    ]);
  });

  it('shows a plain error with Retry, which refetches', async () => {
    mockGet.mockRejectedValueOnce(new Error('boom'));
    const r = await render();
    expect(text(r)).toContain("We couldn't load your day");
    mockGet.mockResolvedValue(full);
    const retry = r.root.findAll(
      (n) => n.props.accessibilityRole === 'button' && typeof n.props.onPress === 'function',
    )[0];
    await act(async () => retry.props.onPress());
    await flush();
    expect(text(r)).toContain('Alex');
  });

  it('keeps cached data and says so when a refresh fails', async () => {
    mockGet.mockResolvedValueOnce(full);
    const r = await render();
    expect(text(r)).not.toContain('Saved on this device');
    mockGet.mockRejectedValue(new Error('offline'));
    const sv = r.root.findByType(ScrollView);
    await act(async () => sv.props.refreshControl.props.onRefresh());
    await flush();
    const t = text(r);
    expect(t).toContain('Saved on this device');
    expect(t).toContain('1,420 / 2,200 kcal');
  });

  it('shows pending-sync and offline captions', async () => {
    mockGet.mockResolvedValue(full);
    mockHooks.pending = 2;
    mockHooks.online = false;
    const r = await render();
    expect(text(r)).toContain('2 entries waiting to sync');
    expect(text(r)).toContain('Saved on this device');
    mockHooks.pending = 1;
    const r2 = await render();
    expect(text(r2)).toContain('1 entry waiting to sync');
  });

  it('adds pending local kcal and minutes to the server numbers', async () => {
    mockGet.mockResolvedValue({
      ...full,
      workout_window: { minutes_today: 10, goal_minutes: 30, fraction: 0.33 },
    });
    mockHooks.totals = { kcal: 300, minutes: 20 };
    const r = await render();
    expect(text(r)).toContain('1,720 / 2,200 kcal');
    expect(text(r)).toContain('30 / 30 min');
  });

  describe('progression', () => {
    const iso = (h: number) => new Date(Date.now() + h * 3600_000 + 60_000).toISOString();
    const prog = {
      tracks: {
        strength: {
          state: 'recovering' as const,
          opens_at: iso(6),
          closes_at: iso(54),
          fraction: 0.7,
          protein_bonus: true,
          level: { level: 3, points: 12, progress: 0.5, value: 62 },
        },
        cardio: {
          state: 'missed' as const,
          opens_at: null,
          closes_at: null,
          fraction: 0,
          level: { level: 1, points: 0, progress: 0, value: 5 },
        },
        focus: { level: { level: 2, points: 5, progress: 0.2, value: 30 } },
      },
      ring: { minutes: 12, goal: 30, closed: false },
      hints: ['Eat something before training for more energy.', 'Two', 'Three'],
      new_personal_bests: [
        {
          exercise_id: 'e',
          exercise_name: 'Squat',
          kind: 'weight' as const,
          value: 100,
          previous: 95,
          weight_kg: 100,
          reps: 5,
        },
      ],
    };

    it('shows window state, focus text, ring, at most two hints and the PB chip', async () => {
      mockGet.mockResolvedValue({ ...full, progression: prog });
      const r = await render();
      const t = text(r);
      expect(t).toContain('Recovering · opens in 6 h');
      expect(t).toContain('Protein bonus');
      expect(t).toContain('Window missed — train to get back on track');
      expect(t).toContain('Level 3/4');
      expect(t).toContain('Level 2/4');
      expect(t).toContain('Grows with meditation, yoga, sports');
      expect(t).toContain('12/30 min');
      expect(t).toContain('Eat something before training for more energy.');
      expect(t).toContain('Two');
      expect(t).not.toContain('Three');
      expect(t).toContain('New personal best');
      await act(async () => byLabel(r, 'New personal best').props.onPress());
      expect(mockPush).toHaveBeenCalledWith('/records');
    });

    it('shows Done for today when the ring is closed and no chip without bests', async () => {
      mockGet.mockResolvedValue({
        ...full,
        progression: { ...prog, ring: { minutes: 40, goal: 30, closed: true }, new_personal_bests: [] },
      });
      const t = text(await render());
      expect(t).toContain('Done for today');
      expect(t).not.toContain('New personal best');
    });

    it('links to the program from the Strength card', async () => {
      mockGet.mockResolvedValue({ ...full, progression: prog });
      const r = await render();
      await act(async () => byLabel(r, 'Program').props.onPress());
      expect(mockPush).toHaveBeenCalledWith('/program');
    });
  });
});
