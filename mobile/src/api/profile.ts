import { authedRequest } from './client';
import type { Units } from './weight';

export type Sex = 'male' | 'female';
export type ActivityLevel = 'sedentary' | 'light' | 'moderate' | 'active' | 'very_active';
export type Goal = 'lose' | 'maintain' | 'gain';
export type CalorieMode = 'auto' | 'manual';

export type BodyProfileFields = {
  height_cm?: number | null;
  birth_date?: string | null;
  sex?: Sex | null;
  activity_level?: ActivityLevel | null;
  goal?: Goal | null;
  goal_rate_kg_per_week?: number | null;
  calorie_target_mode?: CalorieMode | null;
};

export type Profile = {
  display_name: string | null;
  calorie_target: number;
  weight_goal_kg: number | null;
  units: Units;
  updated_at: string | null;
} & BodyProfileFields;

export type ProfileInput = {
  display_name: string | null;
  calorie_target: number;
  weight_goal_kg: number | null;
  units: Units;
} & BodyProfileFields;

export const ACTIVITY_LEVELS: { value: ActivityLevel; label: string; hint: string }[] = [
  { value: 'sedentary', label: 'Sedentary', hint: 'Desk job, little or no exercise' },
  { value: 'light', label: 'Light', hint: 'Light exercise 1-3 days a week' },
  { value: 'moderate', label: 'Moderate', hint: 'Exercise 3-5 days a week' },
  { value: 'active', label: 'Active', hint: 'Hard exercise 6-7 days a week' },
  { value: 'very_active', label: 'Very active', hint: 'Physical job or training twice a day' },
];

export const GOAL_RATES = [0.25, 0.5, 0.75, 1] as const;

export const MIN_HEIGHT_CM = 100;
export const MAX_HEIGHT_CM = 250;

/** Returns an error message, or null when height is empty or valid. */
export function validateHeight(text: string): string | null {
  const v = text.trim().replace(',', '.');
  if (!v) return null;
  const n = Number(v);
  if (!/^\d+(\.\d+)?$/.test(v) || n < MIN_HEIGHT_CM || n > MAX_HEIGHT_CM) {
    return `Use a height between ${MIN_HEIGHT_CM} and ${MAX_HEIGHT_CM} cm.`;
  }
  return null;
}

export function ageOn(birthDate: string, now: Date = new Date()): number {
  const [y, m, d] = birthDate.split('-').map(Number);
  let age = now.getUTCFullYear() - y;
  const month = now.getUTCMonth() + 1;
  if (month < m || (month === m && now.getUTCDate() < d)) age -= 1;
  return age;
}

/** Returns an error message, or null when the birth date is empty or valid (age 13..100). */
export function validateBirthDate(text: string, now: Date = new Date()): string | null {
  const v = text.trim();
  if (!v) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v);
  if (!m) return 'Use the format YYYY-MM-DD.';
  const y = Number(m[1]);
  const mo = Number(m[2]);
  const day = Number(m[3]);
  const d = new Date(Date.UTC(y, mo - 1, day));
  if (d.getUTCFullYear() !== y || d.getUTCMonth() !== mo - 1 || d.getUTCDate() !== day) {
    return 'That date does not exist.';
  }
  const age = ageOn(v, now);
  if (age < 13 || age > 100) return 'Age must be between 13 and 100.';
  return null;
}

export const profileKeys = { all: ['profile'] as const };

export const MIN_CALORIES = 800;
export const MAX_CALORIES = 10000;

/** Returns an error message, or null when the calorie target is valid. */
export function validateCalorieTarget(text: string): string | null {
  const v = text.trim();
  if (!/^\d+$/.test(v)) return 'Enter a whole number of calories.';
  const n = Number(v);
  if (n < MIN_CALORIES || n > MAX_CALORIES) {
    return `Use a value between ${MIN_CALORIES} and ${MAX_CALORIES}.`;
  }
  return null;
}

export function getProfile(): Promise<Profile> {
  return authedRequest<Profile>('/profile');
}

export function saveProfile(input: ProfileInput): Promise<Profile> {
  return authedRequest<Profile>('/profile', 'PUT', input);
}
