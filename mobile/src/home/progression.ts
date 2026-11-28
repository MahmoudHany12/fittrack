import type { Progression, TrackLevel, WindowTrack } from '@/api/progression';

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));
const isNum = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);

/** Time left as "31 h" / "45 min" (under an hour shows minutes). */
export function formatLeft(ms: number): string {
  const min = Math.max(1, Math.ceil(ms / 60000));
  if (min < 60) return `${min} min`;
  return `${Math.round(min / 60)} h`;
}

function msUntil(iso: string | null | undefined, now: number): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? null : t - now;
}

/** Bar fill 0..1: open drains with the time left, recovering fills toward opening. */
export function windowFraction(track: Pick<WindowTrack, 'state' | 'fraction'>): number {
  if (track.state === 'needs_rest' || track.state === 'missed') return 0;
  if (!isNum(track.fraction)) return track.state === 'open' ? 1 : 0;
  return clamp01(track.fraction);
}

export type WindowView = {
  /** Small line under "Level n/4". */
  text: string;
  fraction: number;
  /** Amber caption (missed window). */
  warn: boolean;
  proteinBonus: boolean;
};

export function windowView(track: WindowTrack, now: number = Date.now()): WindowView {
  const base = { fraction: windowFraction(track), warn: false, proteinBonus: false };
  switch (track.state) {
    case 'open': {
      const left = msUntil(track.closes_at, now);
      return {
        ...base,
        text: left != null && left > 0 ? `Window open · ${formatLeft(left)} left` : 'Window open',
      };
    }
    case 'recovering': {
      const left = msUntil(track.opens_at, now);
      return {
        ...base,
        proteinBonus: track.protein_bonus === true,
        text: left != null && left > 0 ? `Recovering · opens in ${formatLeft(left)}` : 'Recovering',
      };
    }
    case 'needs_rest':
      return { ...base, text: 'Sleep to open your next window' };
    case 'missed':
      return { ...base, warn: true, text: 'Window missed — train to get back on track' };
    default:
      return { ...base, text: '' };
  }
}

/** Level 1..4 and ring progress from a progression track. */
export function trackView(
  track: TrackLevel | null | undefined,
): { level: number; progress: number } | null {
  if (!track || !isNum(track.level)) return null;
  return {
    level: Math.min(4, Math.max(1, Math.round(track.level))),
    progress: isNum(track.progress) ? clamp01(track.progress) : 0,
  };
}

export function ringText(ring: Progression['ring']): { label: string; fraction: number } {
  const goal = ring.goal > 0 ? ring.goal : 30;
  const minutes = Math.max(0, Math.round(ring.minutes));
  return {
    label: ring.closed ? 'Done for today' : `${minutes}/${goal} min`,
    fraction: ring.closed ? 1 : clamp01(minutes / goal),
  };
}

/** At most two non-empty hints. */
export function pickHints(hints: string[] | null | undefined): string[] {
  return (hints ?? []).filter((h) => typeof h === 'string' && h.trim()).slice(0, 2);
}

export const FOCUS_TEXT = 'Grows with meditation, yoga, sports';
