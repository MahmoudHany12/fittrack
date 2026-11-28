import { authedRequest } from './client';

export type FoodSource = 'usda' | 'off' | 'custom' | 'builtin';
export type Meal = 'breakfast' | 'lunch' | 'dinner' | 'snack';

/** Nutrient values are per serving (`serving_g` grams). */
export type Food = {
  id: string;
  name: string;
  brand: string | null;
  kcal: number;
  protein_g: number;
  carbs_g: number;
  fat_g: number;
  serving_g: number;
  source: FoodSource;
  external_id: string | null;
  /** Built-in catalog section, e.g. "Egyptian"; null for other sources. */
  category?: string | null;
  /** Optional Arabic name (built-in catalog). */
  name_ar?: string | null;
};

export type FoodCategory = { name: string; count: number };

export type FoodLog = {
  id: string;
  food_id: string;
  food_name: string;
  brand: string | null;
  logged_at: string;
  servings: number;
  kcal: number;
  protein_g: number;
  carbs_g: number;
  fat_g: number;
  meal: Meal | null;
  updated_at: string;
};

export type FoodSummary = {
  date: string;
  kcal: number;
  protein_g: number;
  carbs_g: number;
  fat_g: number;
  entries: number;
};

export type NewFood = Pick<Food, 'name' | 'kcal' | 'protein_g' | 'carbs_g' | 'fat_g' | 'serving_g'> & {
  brand?: string | null;
};

export const MIN_QUERY_LENGTH = 2;
export const MIN_SERVINGS = 0.5;
export const MAX_SERVINGS = 20;
export const SERVING_STEP = 0.5;

export const foodKeys = {
  all: ['food'] as const,
  search: (q: string) => ['food', 'search', q] as const,
  recent: ['food', 'recent'] as const,
  favorites: ['food', 'favorites'] as const,
  categories: ['food', 'categories'] as const,
  catalog: (category: string) => ['food', 'catalog', category] as const,
  logs: (date: string) => ['food', 'logs', date] as const,
  summary: (date: string) => ['food', 'summary', date] as const,
};

/** Client-generated id so a retried POST /food-logs is idempotent. */
export function newId(): string {
  const c = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  if (c?.randomUUID) return c.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (ch) => {
    const r = Math.floor(Math.random() * 16);
    return (ch === 'x' ? r : (r & 0x3) | 0x8).toString(16);
  });
}

/** Today's date as the API understands it (UTC). */
export function todayUtc(now: Date = new Date()): string {
  return now.toISOString().slice(0, 10);
}

export function clampServings(n: number): number {
  if (!Number.isFinite(n)) return 1;
  return Math.min(MAX_SERVINGS, Math.max(MIN_SERVINGS, Math.round(n / SERVING_STEP) * SERVING_STEP));
}

export function stepServings(current: number, direction: 1 | -1): number {
  return clampServings(current + direction * SERVING_STEP);
}

/** What the log will add up to; mirrors the server (rounded to 2 decimals). */
export function previewTotals(
  food: Pick<Food, 'kcal' | 'protein_g' | 'carbs_g' | 'fat_g'>,
  servings: number,
) {
  const r = (v: number) => Math.round(v * servings * 100) / 100;
  return {
    kcal: r(food.kcal),
    protein_g: r(food.protein_g),
    carbs_g: r(food.carbs_g),
    fat_g: r(food.fat_g),
  };
}

export function formatServing(food: Pick<Food, 'serving_g'>): string {
  return `${Math.round(food.serving_g * 10) / 10} g`;
}

/** Search needs at least 2 non-space characters; shorter queries never hit the API. */
export function isSearchable(q: string): boolean {
  return q.trim().length >= MIN_QUERY_LENGTH;
}

export function searchFoods(q: string): Promise<Food[]> {
  return authedRequest<Food[]>(`/foods/search?q=${encodeURIComponent(q.trim())}`);
}

export function recentFoods(): Promise<Food[]> {
  return authedRequest<Food[]>('/foods/recent');
}

/** Category chip selected when the food screen opens. */
export const DEFAULT_CATEGORY = 'Egyptian';

export function listCategories(): Promise<FoodCategory[]> {
  return authedRequest<FoodCategory[]>('/foods/categories');
}

/** Built-in catalog foods, ordered by category then name; all categories when omitted. */
export function listCatalog(category?: string): Promise<Food[]> {
  const c = category?.trim();
  return authedRequest<Food[]>(c ? `/foods/catalog?category=${encodeURIComponent(c)}` : '/foods/catalog');
}

/** The default category first, then the rest in server (alphabetical) order. */
export function orderCategories(cats: FoodCategory[], first: string = DEFAULT_CATEGORY): FoodCategory[] {
  const head = cats.filter((c) => c.name === first);
  return [...head, ...cats.filter((c) => c.name !== first)];
}

/** Keeps the chosen category if it still exists, else falls back to the first chip. */
export function pickCategory(selected: string | null, ordered: FoodCategory[]): string {
  if (selected && ordered.some((c) => c.name === selected)) return selected;
  return ordered[0]?.name ?? DEFAULT_CATEGORY;
}

const round1 = (v: number) => Math.round(v * 10) / 10;

/** "P 17 · C 98 · F 11" per serving, one decimal at most. */
export function formatMacros(food: Pick<Food, 'protein_g' | 'carbs_g' | 'fat_g'>): string {
  return `P ${round1(food.protein_g)} · C ${round1(food.carbs_g)} · F ${round1(food.fat_g)}`;
}

export function createFood(food: NewFood): Promise<Food> {
  return authedRequest<Food>('/foods', 'POST', food);
}

export function listFoodLogs(date: string): Promise<FoodLog[]> {
  return authedRequest<FoodLog[]>(`/food-logs?date=${encodeURIComponent(date)}`);
}

export function getFoodSummary(date: string): Promise<FoodSummary> {
  return authedRequest<FoodSummary>(`/food-logs/summary?date=${encodeURIComponent(date)}`);
}

export type NewFoodLog = {
  /** Reuse the same id when retrying so the server de-duplicates. */
  id?: string;
  food_id: string;
  servings: number;
  logged_at?: string;
  meal?: Meal;
};

export function createFoodLog(log: NewFoodLog): Promise<FoodLog> {
  return authedRequest<FoodLog>('/food-logs', 'POST', { ...log, id: log.id ?? newId() });
}

export function updateFoodLog(
  id: string,
  patch: { servings?: number; logged_at?: string; meal?: Meal },
): Promise<FoodLog> {
  return authedRequest<FoodLog>(`/food-logs/${encodeURIComponent(id)}`, 'PUT', patch);
}

export function deleteFoodLog(id: string): Promise<void> {
  return authedRequest<void>(`/food-logs/${encodeURIComponent(id)}`, 'DELETE');
}

export function listFavorites(): Promise<Food[]> {
  return authedRequest<Food[]>('/favorites');
}

export function addFavorite(foodId: string): Promise<void> {
  return authedRequest<void>(`/favorites/${encodeURIComponent(foodId)}`, 'PUT');
}

export function removeFavorite(foodId: string): Promise<void> {
  return authedRequest<void>(`/favorites/${encodeURIComponent(foodId)}`, 'DELETE');
}
