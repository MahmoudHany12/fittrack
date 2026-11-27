import { authedRequest } from './client';
import { formatDuration } from './sleep';
import { formatWeight, type Units } from './weight';

export type WeeklySummary = {
  from: string;
  to: string;
  training: {
    workouts: number | null;
    minutes: number | null;
    active_days: number | null;
    vs_prev_week_minutes: number | null;
  };
  sleep: {
    avg_minutes: number | null;
    avg_quality: number | null;
    nights: number | null;
    vs_prev_week_minutes: number | null;
  };
  weight: {
    start_kg: number | null;
    end_kg: number | null;
    change_kg: number | null;
    trend: 'up' | 'flat' | 'down' | null;
  };
  diet: {
    avg_kcal: number | null;
    days_logged: number | null;
    target: number | null;
  };
};

export const summaryKeys = { weekly: (weekStart?: string) => ['summary', 'weekly', weekStart ?? 'latest'] as const };

/** `weekStart` is a YYYY-MM-DD date; without it the API uses the 7 days ending today. */
export function getWeeklySummary(weekStart?: string): Promise<WeeklySummary> {
  const q = weekStart ? `?week_start=${encodeURIComponent(weekStart)}` : '';
  return authedRequest<WeeklySummary>(`/summary/weekly${q}`);
}

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many);

/** "up from last week", "down from last week", or "the same as last week". */
function comparison(diff: number | null, unit: (n: number) => string): string | null {
  if (diff === null) return null;
  if (diff === 0) return 'the same as last week';
  return `${diff > 0 ? 'up' : 'down'} ${unit(Math.abs(diff))} from last week`;
}

export function trainingSentence(t: WeeklySummary['training']): string {
  if (t.workouts === null || t.minutes === null) return 'No workouts logged this week.';
  const base = `You trained ${t.workouts} ${plural(t.workouts, 'time', 'times')}, ${t.minutes} min`;
  const cmp =
    t.vs_prev_week_minutes === null
      ? null
      : t.vs_prev_week_minutes === 0
        ? 'the same as last week'
        : `${t.vs_prev_week_minutes > 0 ? 'up' : 'down'} from last week`;
  return cmp ? `${base} — ${cmp}` : base;
}

export function sleepSentence(s: WeeklySummary['sleep']): string {
  if (s.avg_minutes === null || s.nights === null) return 'No sleep logged this week.';
  const base = `You slept ${formatDuration(s.avg_minutes)} a night on average (${s.nights} ${plural(s.nights, 'night', 'nights')} logged)`;
  const cmp = comparison(s.vs_prev_week_minutes, (n) => formatDuration(n));
  return cmp ? `${base} — ${cmp}` : base;
}

export function weightSentence(w: WeeklySummary['weight'], units: Units = 'metric'): string {
  if (w.start_kg === null || w.end_kg === null || w.change_kg === null) {
    return 'No weigh-ins this week.';
  }
  if (w.trend === 'flat' || w.trend === null) {
    return `Your weight stayed steady at ${formatWeight(w.end_kg, units)}.`;
  }
  const delta = formatWeight(Math.abs(w.change_kg), units);
  return `Your weight went ${w.trend} ${delta}, from ${formatWeight(w.start_kg, units)} to ${formatWeight(w.end_kg, units)}.`;
}

export function dietSentence(d: WeeklySummary['diet']): string {
  if (d.avg_kcal === null || d.days_logged === null) return 'No food logged this week.';
  const days = `${d.days_logged} ${plural(d.days_logged, 'day', 'days')}`;
  const base = `You logged food on ${days}, about ${d.avg_kcal.toLocaleString('en-US')} kcal a day`;
  return d.target === null ? `${base}.` : `${base} (target ${d.target.toLocaleString('en-US')}).`;
}

/** Whether nothing at all was logged, so the screen can show one friendly empty state. */
export function isEmptyWeek(s: WeeklySummary): boolean {
  return (
    s.training.workouts === null &&
    s.sleep.nights === null &&
    s.weight.start_kg === null &&
    s.diet.days_logged === null
  );
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "9 Mar – 15 Mar" from two YYYY-MM-DD dates. */
export function formatRange(from: string, to: string): string {
  const f = (d: string) => {
    const [, m, day] = d.split('-').map(Number);
    return `${day} ${MONTHS[(m ?? 1) - 1]}`;
  };
  return `${f(from)} – ${f(to)}`;
}
