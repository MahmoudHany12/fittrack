import type { StatusKey, Trend } from '@/api/dashboard';
import type { Palette } from '@/theme';

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));
const isNum = (n: unknown): n is number => typeof n === 'number' && Number.isFinite(n);

/** 0-100 stat value -> level 1..4 (0-24 -> 1 ... 75-100 -> 4). Null/invalid reads as level 1. */
export function levelFromValue(value: number | null | undefined): number {
  if (!isNum(value)) return 1;
  return clamp(Math.floor(clamp(value, 0, 100) / 25) + 1, 1, 4);
}

/** Progress (0..1) inside the current level. Full ring at 100. */
export function levelProgress(value: number | null | undefined): number {
  if (!isNum(value)) return 0;
  const v = clamp(value, 0, 100);
  if (v >= 100) return 1;
  return (v % 25) / 25;
}

/** Prefer server-provided level/progress, fall back to deriving from value. */
export function statView(
  stat: { value: number | null; level: number | null; progress: number | null } | null | undefined,
): { level: number; progress: number } {
  if (!stat) return { level: 1, progress: 0 };
  const level = isNum(stat.level) ? clamp(Math.round(stat.level), 1, 4) : levelFromValue(stat.value);
  const progress = isNum(stat.progress) ? clamp(stat.progress, 0, 1) : levelProgress(stat.value);
  return { level, progress };
}

/** kcal_eaten / kcal_target clamped to 0..1; 0 when either is missing or target <= 0. */
export function kcalFraction(
  eaten: number | null | undefined,
  target: number | null | undefined,
): number {
  if (!isNum(eaten) || !isNum(target) || target <= 0) return 0;
  return clamp(eaten / target, 0, 1);
}

export function statusColor(key: StatusKey | string | null | undefined, p: Palette): string {
  if (key === 'well_rested') return p.good;
  if (key === 'okay' || key === 'tired') return p.warn;
  return p.muted;
}

export function dietColor(label: string | null | undefined, p: Palette): string {
  if (label === 'On track') return p.good;
  if (label === 'Under-eating' || label === 'Overeating') return p.warn;
  return p.muted;
}

export function trendArrow(trend: Trend | string | null | undefined): string {
  if (trend === 'up') return '↗';
  if (trend === 'down') return '↘';
  if (trend === 'flat') return '→';
  return '';
}

export function trendWord(trend: Trend | string | null | undefined): string {
  if (trend === 'up') return 'trending up';
  if (trend === 'down') return 'trending down';
  if (trend === 'flat') return 'steady';
  return '';
}

/** 460 -> "7 h 40 min", 480 -> "8 h", 45 -> "45 min". Invalid -> "0 min". */
export function formatHours(totalMinutes: number | null | undefined): string {
  const m = isNum(totalMinutes) ? Math.max(0, Math.round(totalMinutes)) : 0;
  const h = Math.floor(m / 60);
  const min = m % 60;
  if (h === 0) return `${min} min`;
  if (min === 0) return `${h} h`;
  return `${h} h ${min} min`;
}

/** 1420 -> "1,420". */
export function formatNumber(n: number | null | undefined): string {
  if (!isNum(n)) return '0';
  return String(Math.round(n)).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** Home status word: readiness score first (>=70 Healthy, 40-69 Okay, <40 Tired), else the status key. */
export function statusWord(
  key: StatusKey | string | null | undefined,
  score?: number | null,
): 'Healthy' | 'Okay' | 'Tired' | 'No data yet' {
  if (isNum(score)) return score >= 70 ? 'Healthy' : score >= 40 ? 'Okay' : 'Tired';
  if (key === 'well_rested') return 'Healthy';
  if (key === 'okay') return 'Okay';
  if (key === 'tired') return 'Tired';
  return 'No data yet';
}

export function statusWordColor(word: string, p: Palette): string {
  if (word === 'Healthy') return p.good;
  if (word === 'Okay' || word === 'Tired') return p.warn;
  return p.muted;
}

/** "Alex Smith" -> "AS", "alex" -> "A", null -> "?". */
export function initials(name: string | null | undefined): string {
  const parts = (name ?? '').trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  const first = parts[0][0];
  const last = parts.length > 1 ? parts[parts.length - 1][0] : '';
  return (first + last).toUpperCase();
}

/** Prefer a still-unsynced local weight (or one when the server has none) over the dashboard's. */
export function pickWeight(
  server: number | null | undefined,
  local: { kg: number; pending: boolean } | null | undefined,
): number | null {
  if (local && (local.pending || !isNum(server))) return local.kg;
  return isNum(server) ? server : null;
}

/** 78.4 -> "78.4 kg". */
export function formatKg(kg: number): string {
  return `${Math.round(kg * 10) / 10} kg`;
}
