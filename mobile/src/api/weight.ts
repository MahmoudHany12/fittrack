import { authedRequest } from './client';

export type Units = 'metric' | 'imperial';

export type WeightLog = {
  id: string;
  measured_at: string;
  kg: number;
  updated_at: string;
  created_at: string;
};

export const MIN_KG = 20;
export const MAX_KG = 400;
const KG_PER_LB = 0.45359237;

export const weightKeys = { all: ['weight'] as const };

export function kgToLb(kg: number): number {
  return kg / KG_PER_LB;
}

export function lbToKg(lb: number): number {
  return lb * KG_PER_LB;
}

/** Value shown in an input or label for the chosen display units (1 decimal). */
export function displayWeight(kg: number, units: Units): string {
  const v = units === 'imperial' ? kgToLb(kg) : kg;
  return (Math.round(v * 10) / 10).toString();
}

export function unitLabel(units: Units): string {
  return units === 'imperial' ? 'lb' : 'kg';
}

export function formatWeight(kg: number, units: Units): string {
  return `${displayWeight(kg, units)} ${unitLabel(units)}`;
}

/** Parses typed text (accepts a decimal comma) into kilograms, or null if invalid / out of range. */
export function parseWeightInput(text: string, units: Units): number | null {
  const cleaned = text.trim().replace(',', '.');
  if (!/^\d+(\.\d+)?$/.test(cleaned)) return null;
  const value = Number(cleaned);
  const kg = Math.round((units === 'imperial' ? lbToKg(value) : value) * 100) / 100;
  return kg >= MIN_KG && kg <= MAX_KG ? kg : null;
}

/** Client-generated id so a retried save is idempotent on the server. */
export function newId(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16);
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

export function listWeight(range: { from?: string; to?: string } = {}): Promise<WeightLog[]> {
  const q = new URLSearchParams();
  if (range.from) q.set('from', range.from);
  if (range.to) q.set('to', range.to);
  const qs = q.toString();
  return authedRequest<WeightLog[]>(`/weight${qs ? `?${qs}` : ''}`);
}

export function logWeight(input: { id?: string; kg: number; measured_at?: string }): Promise<WeightLog> {
  return authedRequest<WeightLog>('/weight', 'POST', input);
}

export function updateWeight(
  id: string,
  input: { kg: number; measured_at?: string },
): Promise<WeightLog> {
  return authedRequest<WeightLog>(`/weight/${encodeURIComponent(id)}`, 'PUT', input);
}

export function deleteWeight(id: string): Promise<void> {
  return authedRequest<void>(`/weight/${encodeURIComponent(id)}`, 'DELETE');
}
