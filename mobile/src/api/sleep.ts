import { authedRequest } from './client';

export type SleepLog = {
  id: string;
  bedtime: string;
  wake_time: string;
  quality: number | null;
  source: string;
  external_id: string | null;
  duration_minutes: number;
  updated_at: string;
  created_at: string;
};

export type SleepInput = {
  /** Client-generated id makes retries idempotent. */
  id?: string;
  bedtime: string;
  wake_time: string;
  quality?: number | null;
};

export type SleepRange = { from?: Date; to?: Date };

export const MAX_SLEEP_MINUTES = 24 * 60;

export function listSleep(range: SleepRange = {}): Promise<SleepLog[]> {
  const q: string[] = [];
  if (range.from) q.push(`from=${encodeURIComponent(range.from.toISOString())}`);
  if (range.to) q.push(`to=${encodeURIComponent(range.to.toISOString())}`);
  return authedRequest<SleepLog[]>(`/sleep${q.length ? `?${q.join('&')}` : ''}`);
}

/** Idempotent upsert: posting the same `id` again updates the same row. */
export function saveSleep(input: SleepInput): Promise<SleepLog> {
  return authedRequest<SleepLog>('/sleep', 'POST', input);
}

export function updateSleep(
  id: string,
  input: Pick<SleepInput, 'bedtime' | 'wake_time' | 'quality'>,
): Promise<SleepLog> {
  return authedRequest<SleepLog>(`/sleep/${encodeURIComponent(id)}`, 'PUT', input);
}

export function deleteSleep(id: string): Promise<void> {
  return authedRequest<void>(`/sleep/${encodeURIComponent(id)}`, 'DELETE');
}

/** Random v4 UUID (not security sensitive: it only identifies the user's own row). */
export function newSleepId(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16);
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

/** "7 h 40 min", "8 h", "45 min". Negative or non-finite values read as "0 min". */
export function formatDuration(totalMinutes: number): string {
  const m = Number.isFinite(totalMinutes) ? Math.max(0, Math.round(totalMinutes)) : 0;
  const h = Math.floor(m / 60);
  const min = m % 60;
  if (h === 0) return `${min} min`;
  if (min === 0) return `${h} h`;
  return `${h} h ${min} min`;
}

/** Whole minutes between two instants (wake - bed); may be <= 0 for invalid input. */
export function durationMinutes(bedtime: Date, wake: Date): number {
  return Math.round((wake.getTime() - bedtime.getTime()) / 60000);
}

/** Whether the pair would be accepted by the API (wake after bed, at most 24 h). */
export function isValidWindow(bedtime: Date, wake: Date): boolean {
  const d = durationMinutes(bedtime, wake);
  return d > 0 && d <= MAX_SLEEP_MINUTES;
}

/**
 * Prefill for the quick log: last night, 23:00 -> 07:00 local time.
 * Wake is the most recent 07:00 that is not in the future.
 */
export function defaultSleepWindow(now: Date): { bedtime: Date; wake: Date } {
  const wake = new Date(now);
  wake.setHours(7, 0, 0, 0);
  if (wake.getTime() > now.getTime()) wake.setDate(wake.getDate() - 1);
  const bedtime = new Date(wake);
  bedtime.setHours(23, 0, 0, 0);
  bedtime.setDate(bedtime.getDate() - 1);
  return { bedtime, wake };
}

/** Move an instant by whole minutes. */
export function shiftMinutes(d: Date, minutes: number): Date {
  return new Date(d.getTime() + minutes * 60000);
}

const pad = (n: number) => String(n).padStart(2, '0');

/** "Yesterday 23:00", "Today 07:00", or "12 Mar 23:00" relative to `now`. */
export function formatMoment(d: Date, now: Date): string {
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOfDay(now) - startOfDay(d)) / 86400000);
  const clock = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  if (days === 0) return `Today ${clock}`;
  if (days === 1) return `Yesterday ${clock}`;
  if (days === -1) return `Tomorrow ${clock}`;
  const month = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][
    d.getMonth()
  ];
  return `${d.getDate()} ${month} ${clock}`;
}
