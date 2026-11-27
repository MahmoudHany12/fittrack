import { authedRequest } from './client';
import type { Units } from './weight';

export type Profile = {
  display_name: string | null;
  calorie_target: number;
  weight_goal_kg: number | null;
  units: Units;
  updated_at: string | null;
};

export type ProfileInput = {
  display_name: string | null;
  calorie_target: number;
  weight_goal_kg: number | null;
  units: Units;
};

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
