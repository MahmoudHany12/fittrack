import { authedRequest } from './client';
import {
  barFraction,
  getLeaderboard,
  getOptIn,
  setOptIn,
  totalLabel,
  type LeaderboardEntry,
} from './leaderboard';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

beforeEach(() => {
  mockReq.mockReset();
  mockReq.mockResolvedValue({});
});

const entry = (total: number | null): LeaderboardEntry => ({
  user_id: 'u',
  display_name: 'Ann',
  is_me: true,
  rank: 1,
  strength: null,
  cardio: null,
  focus: null,
  total,
});

describe('leaderboard api', () => {
  it('reads and writes the opt-in', async () => {
    await getOptIn();
    expect(mockReq).toHaveBeenCalledWith('/leaderboard/optin');
    await setOptIn(true);
    expect(mockReq).toHaveBeenLastCalledWith('/leaderboard/optin', 'PUT', { opted_in: true });
    await setOptIn(false);
    expect(mockReq).toHaveBeenLastCalledWith('/leaderboard/optin', 'PUT', { opted_in: false });
  });

  it('loads the board', async () => {
    await getLeaderboard();
    expect(mockReq).toHaveBeenCalledWith('/leaderboard');
  });
});

describe('helpers', () => {
  it('clamps bar fractions', () => {
    expect(barFraction(null)).toBe(0);
    expect(barFraction({ value: 40, level: 2, progress: 0.6 })).toBeCloseTo(0.4);
    expect(barFraction({ value: 140, level: 4, progress: 1 })).toBe(1);
  });

  it('labels totals', () => {
    expect(totalLabel(entry(75))).toBe('75 total');
    expect(totalLabel(entry(null))).toBe('No stats yet');
  });
});
