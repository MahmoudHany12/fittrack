import { authedRequest } from './client';

export type StatValue = { value: number; level: number; progress: number };

/** Only a display name and stat values: never an email. Stats are null when nothing is logged. */
export type LeaderboardEntry = {
  user_id: string;
  display_name: string;
  is_me: boolean;
  rank: number;
  strength: StatValue | null;
  cardio: StatValue | null;
  focus: StatValue | null;
  total: number | null;
};

export const leaderboardKeys = {
  /** Prefix of every leaderboard query (board and opt-in). */
  board: ['leaderboard'] as const,
  optin: ['leaderboard', 'optin'] as const,
};

export function getOptIn(): Promise<{ opted_in: boolean }> {
  return authedRequest<{ opted_in: boolean }>('/leaderboard/optin');
}

export function setOptIn(optedIn: boolean): Promise<{ opted_in: boolean }> {
  return authedRequest<{ opted_in: boolean }>('/leaderboard/optin', 'PUT', {
    opted_in: optedIn,
  });
}

/** 403 (ApiError) until the caller has opted in. */
export function getLeaderboard(): Promise<LeaderboardEntry[]> {
  return authedRequest<LeaderboardEntry[]>('/leaderboard');
}

/** Bar fill 0..1 for a 0-100 stat value (empty when missing). */
export function barFraction(stat: StatValue | null): number {
  if (!stat) return 0;
  return Math.max(0, Math.min(1, stat.value / 100));
}

export function totalLabel(entry: LeaderboardEntry): string {
  return entry.total === null ? 'No stats yet' : `${entry.total} total`;
}
