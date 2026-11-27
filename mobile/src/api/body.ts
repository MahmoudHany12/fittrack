import { authedRequest } from './client';

export type BodyComposition = {
  id: string;
  measured_at: string;
  weight_kg: number;
  body_fat_pct: number | null;
  skeletal_muscle_kg: number | null;
  fat_mass_kg: number | null;
  visceral_fat_level: number | null;
  bmr_kcal: number | null;
  body_water_pct: number | null;
  source: 'inbody' | 'manual';
  updated_at: string;
};

export type BodyCompositionInput = {
  id: string;
  measured_at: string;
  weight_kg: number;
  body_fat_pct: number | null;
  skeletal_muscle_kg: number | null;
  fat_mass_kg: number | null;
  visceral_fat_level: number | null;
  bmr_kcal: number | null;
  body_water_pct: number | null;
  source: 'inbody' | 'manual';
};

export const bodyKeys = { all: ['body'] as const };

type Range = { min: number; max: number; integer?: boolean; label: string; unit: string };

export const BODY_RANGES = {
  weight_kg: { min: 20, max: 400, label: 'Weight', unit: 'kg' },
  body_fat_pct: { min: 2, max: 75, label: 'Body fat', unit: '%' },
  skeletal_muscle_kg: { min: 5, max: 100, label: 'Skeletal muscle mass', unit: 'kg' },
  fat_mass_kg: { min: 1, max: 300, label: 'Fat mass', unit: 'kg' },
  visceral_fat_level: { min: 1, max: 30, integer: true, label: 'Visceral fat level', unit: '' },
  bmr_kcal: { min: 600, max: 5000, integer: true, label: 'BMR', unit: 'kcal' },
  body_water_pct: { min: 20, max: 80, label: 'Body water', unit: '%' },
} as const satisfies Record<string, Range>;

export type BodyField = keyof typeof BODY_RANGES;

/** Parses a typed number (decimal comma ok). Returns null when not a plain number. */
export function parseNumber(text: string): number | null {
  const cleaned = text.trim().replace(',', '.');
  if (!/^\d+(\.\d+)?$/.test(cleaned)) return null;
  return Number(cleaned);
}

/** Plain-language error for a field, or null when valid. Empty is valid only when optional. */
export function validateBodyField(field: BodyField, text: string, required = false): string | null {
  const r: Range = BODY_RANGES[field];
  if (!text.trim()) return required ? `Enter your ${r.label.toLowerCase()}.` : null;
  const n = parseNumber(text);
  const unit = r.unit ? ` ${r.unit}` : '';
  const range = `Use a number between ${r.min} and ${r.max}${unit}.`;
  if (n === null) return range;
  if (r.integer && !Number.isInteger(n)) return `Use a whole number between ${r.min} and ${r.max}.`;
  if (n < r.min || n > r.max) return range;
  return null;
}

/** Validates a YYYY-MM-DD date that is not in the future. */
export function validateScanDate(text: string, now: Date = new Date()): string | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text.trim());
  if (!m) return 'Use the format YYYY-MM-DD.';
  const y = Number(m[1]);
  const mo = Number(m[2]);
  const day = Number(m[3]);
  const d = new Date(Date.UTC(y, mo - 1, day));
  if (d.getUTCFullYear() !== y || d.getUTCMonth() !== mo - 1 || d.getUTCDate() !== day) {
    return 'That date does not exist.';
  }
  if (d.getTime() > now.getTime() + 24 * 3600 * 1000) return 'The date cannot be in the future.';
  return null;
}

export type BodyForm = Record<BodyField, string> & { date: string };
export type BodyFormErrors = Partial<Record<BodyField | 'date', string>>;

export function validateBodyForm(f: BodyForm, now: Date = new Date()): BodyFormErrors {
  const errors: BodyFormErrors = {};
  const date = validateScanDate(f.date, now);
  if (date) errors.date = date;
  (Object.keys(BODY_RANGES) as BodyField[]).forEach((k) => {
    const e = validateBodyField(k, f[k], k === 'weight_kg');
    if (e) errors[k] = e;
  });
  return errors;
}

function opt(text: string): number | null {
  return text.trim() ? parseNumber(text) : null;
}

/** Builds the POST body from a form that already passed validateBodyForm. */
export function buildBodyInput(f: BodyForm, id: string, now: Date = new Date()): BodyCompositionInput {
  const today = now.toISOString().slice(0, 10);
  const date = f.date.trim();
  const measured_at = date === today ? now.toISOString() : `${date}T12:00:00.000Z`;
  const hasScan = [
    f.body_fat_pct,
    f.skeletal_muscle_kg,
    f.fat_mass_kg,
    f.visceral_fat_level,
    f.bmr_kcal,
    f.body_water_pct,
  ].some((v) => v.trim() !== '');
  return {
    id,
    measured_at,
    weight_kg: parseNumber(f.weight_kg) ?? 0,
    body_fat_pct: opt(f.body_fat_pct),
    skeletal_muscle_kg: opt(f.skeletal_muscle_kg),
    fat_mass_kg: opt(f.fat_mass_kg),
    visceral_fat_level: opt(f.visceral_fat_level),
    bmr_kcal: opt(f.bmr_kcal),
    body_water_pct: opt(f.body_water_pct),
    source: hasScan ? 'inbody' : 'manual',
  };
}

export function listBody(range: { from?: string; to?: string } = {}): Promise<BodyComposition[]> {
  const q = new URLSearchParams();
  if (range.from) q.set('from', range.from);
  if (range.to) q.set('to', range.to);
  const qs = q.toString();
  return authedRequest<BodyComposition[]>(`/body-composition${qs ? `?${qs}` : ''}`);
}

export function saveBody(input: BodyCompositionInput): Promise<BodyComposition> {
  return authedRequest<BodyComposition>('/body-composition', 'POST', input);
}
