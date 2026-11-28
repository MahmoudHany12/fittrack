import { buildSections, groupFilters, groupLabel } from "./exerciseBrowse";
import type { Exercise } from "./workouts";

const ex = (name: string, muscle_group: Exercise["muscle_group"]): Exercise => ({
  id: name,
  name,
  muscle_group,
  custom: false,
});

const list = [
  ex("Squat", "legs"),
  ex("Bench Press", "chest"),
  ex("Running", "cardio"),
  ex("Lunge", "legs"),
  ex("Incline Bench Press", "chest"),
];

describe("exerciseBrowse", () => {
  it("builds filters from the data in canonical order", () => {
    expect(groupFilters(list)).toEqual(["all", "chest", "legs", "cardio"]);
  });

  it("groups all exercises, sorted by name inside each section", () => {
    const s = buildSections(list, "all", "");
    expect(s.map((x) => x.title)).toEqual(["Chest", "Legs", "Cardio"]);
    expect(s[0].data.map((e) => e.name)).toEqual(["Bench Press", "Incline Bench Press"]);
    expect(s[1].data.map((e) => e.name)).toEqual(["Lunge", "Squat"]);
  });

  it("filters by group and by text, dropping empty sections", () => {
    expect(buildSections(list, "legs", "").map((x) => x.group)).toEqual(["legs"]);
    const s = buildSections(list, "all", " BENCH ");
    expect(s).toHaveLength(1);
    expect(s[0].data).toHaveLength(2);
    expect(buildSections(list, "cardio", "bench")).toEqual([]);
  });

  it("does not mutate its input", () => {
    const copy = [...list];
    buildSections(list, "all", "");
    expect(list).toEqual(copy);
  });

  it("labels groups", () => {
    expect(groupLabel("focus")).toBe("Focus");
  });
});
