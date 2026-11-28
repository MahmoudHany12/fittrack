import { authedRequest } from './client';

export type BmrSource = 'inbody' | 'katch_mcardle' | 'mifflin';
export type TdeeSource = 'formula' | 'adaptive' | 'blended';

export type Targets = {
  bmr: number | null;
  bmr_source: BmrSource | null;
  tdee: number | null;
  tdee_source: TdeeSource | null;
  calorie_target: number;
  calorie_target_source: 'auto' | 'manual' | 'default';
  protein_g: number | null;
  fat_g: number | null;
  carbs_g: number | null;
  lean_mass_kg: number | null;
  body_fat_pct: number | null;
  missing: string[];
};

export const targetsKeys = { all: ['targets'] as const };

export function getTargets(): Promise<Targets> {
  return authedRequest<Targets>('/targets');
}

export function bmrSourceText(s: BmrSource | null): string {
  switch (s) {
    case 'inbody':
      return 'Measured by your InBody scan';
    case 'katch_mcardle':
      return 'Estimated from your lean mass';
    case 'mifflin':
      return 'Estimated from height, age and sex';
    default:
      return 'Not enough information yet';
  }
}

export function tdeeSourceText(s: TdeeSource | null): string {
  switch (s) {
    case 'adaptive':
      return 'Adapted to your food and weight logs';
    case 'blended':
      return 'A blend of the formula and your food and weight logs';
    case 'formula':
      return 'Formula only. It starts adapting after about two weeks of food and weight logs';
    default:
      return 'Not enough information yet';
  }
}

export function calorieSourceText(s: Targets['calorie_target_source']): string {
  switch (s) {
    case 'auto':
      return 'Worked out from your body and goal';
    case 'manual':
      return 'Set by you in Settings';
    default:
      return 'Default value. Add your body details for a personal target';
  }
}

export type MissingHint = { text: string; to: '/(tabs)/settings' | '/add/body' };

const SCAN = 'Add a body scan for body fat and a measured BMR';

const MISSING: Record<string, MissingHint> = {
  height_cm: { text: 'Add your height', to: '/(tabs)/settings' },
  birth_date: { text: 'Add your birth date', to: '/(tabs)/settings' },
  sex: { text: 'Choose your sex', to: '/(tabs)/settings' },
  activity_level: { text: 'Choose your activity level', to: '/(tabs)/settings' },
  goal: { text: 'Choose your goal', to: '/(tabs)/settings' },
  weight: { text: 'Log your weight', to: '/add/body' },
  weight_kg: { text: 'Log your weight', to: '/add/body' },
  body_fat_pct: { text: SCAN, to: '/add/body' },
  body_composition: { text: SCAN, to: '/add/body' },
  bmr_kcal: { text: SCAN, to: '/add/body' },
};

export function missingHint(key: string): MissingHint {
  return MISSING[key] ?? { text: `Add ${key.replace(/_/g, ' ')}`, to: '/(tabs)/settings' };
}
