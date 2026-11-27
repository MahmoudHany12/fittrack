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

const full: Dashboard = {
  name: 'Alex',
  status: { key: 'well_rested', label: 'Well rested' },
  readiness: { score: 82 },
  sleep: { minutes: 460, quality: 4, label: 'Well rested' },
  diet: { kcal_eaten: 1420, kcal_target: 2200, label: 'On track' },
  exercise: { steps: 6200, workout_minutes_today: 30, workout_count_today: 1 },
  weight: { kg: 78.4, trend: 'down' },
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
    expect(t).toContain('Well rested');
    expect(t).toContain('Strength');
    expect(t).toContain('Level 2/4');
    expect(t).toContain('Level 4/4');
    expect(t).toContain('7 h 40 min');
    expect(t).toContain('1,420 / 2,200 kcal');
    expect(t).toContain('78.4 kg');
    expect(t).toContain('↘');
    expect(t).not.toContain('Activity');
  });

  it('falls back to a single Activity card without stats', async () => {
    mockGet.mockResolvedValue({ ...full, stats: null });
    const r = await render();
    const t = text(r);
    expect(t).toContain('Activity');
    expect(t).toContain('6,200 steps');
    expect(t).not.toContain('Level 2/4');
  });

  it('shows friendly empty copy', async () => {
    mockGet.mockResolvedValue(empty);
    const r = await render();
    const t = text(r);
    expect(t).toContain('No sleep logged yet — tap to add');
    expect(t).toContain('No meals logged yet — tap to add');
    expect(t).toContain('No weight logged yet — tap to add');
    expect(t).toContain('No activity yet — tap to add');
  });

  it('navigates with one tap', async () => {
    mockGet.mockResolvedValue(full);
    const r = await render();
    await act(async () => byLabel(r, 'Sleep').props.onPress());
    await act(async () => byLabel(r, 'Diet').props.onPress());
    await act(async () => byLabel(r, 'Weight').props.onPress());
    await act(async () => byLabel(r, 'Strength').props.onPress());
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
});
