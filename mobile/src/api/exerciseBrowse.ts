import { MUSCLE_GROUPS, type Exercise, type MuscleGroup } from "./workouts";

export type GroupFilter = "all" | MuscleGroup;

export type ExerciseSection = { group: MuscleGroup; title: string; data: Exercise[] };

export function groupLabel(group: MuscleGroup): string {
  return group.charAt(0).toUpperCase() + group.slice(1);
}

/** Filter chips: "All" plus the groups that actually have exercises, in canonical order. */
export function groupFilters(exercises: Exercise[]): GroupFilter[] {
  const present = new Set(exercises.map((e) => e.muscle_group));
  return ["all", ...MUSCLE_GROUPS.filter((g) => present.has(g))];
}

/**
 * Filters by group and name, then groups into sections ordered like MUSCLE_GROUPS
 * with exercises sorted by name inside each section. Empty sections are dropped.
 */
export function buildSections(
  exercises: Exercise[],
  filter: GroupFilter,
  query: string,
): ExerciseSection[] {
  const q = query.trim().toLowerCase();
  const byGroup = new Map<MuscleGroup, Exercise[]>();
  for (const e of exercises) {
    if (filter !== "all" && e.muscle_group !== filter) continue;
    if (q && !e.name.toLowerCase().includes(q)) continue;
    const list = byGroup.get(e.muscle_group);
    if (list) list.push(e);
    else byGroup.set(e.muscle_group, [e]);
  }
  return MUSCLE_GROUPS.filter((g) => byGroup.has(g)).map((group) => ({
    group,
    title: groupLabel(group),
    data: (byGroup.get(group) ?? []).sort((a, b) => a.name.localeCompare(b.name)),
  }));
}
