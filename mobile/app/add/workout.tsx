import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Pressable, ScrollView, SectionList, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  buildSections,
  groupFilters,
  groupLabel,
  type GroupFilter,
} from '@/api/exerciseBrowse';
import { parsePrefill } from '@/api/program';
import { PROTEIN_TIP, REST_MESSAGE, bestLine, getProgression, type PersonalBest } from '@/api/progression';
import {
  LIMITS,
  MUSCLE_GROUPS,
  clamp,
  createExercise,
  isTimeOnly,
  listWorkouts,
  newId,
  searchExercises,
  type Exercise,
  type MuscleGroup,
} from '@/api/workouts';
import { saveWorkout } from '@/sync/store';
import { goBack, refreshAfterSave } from '@/sync/hooks';
import { RecentEntries } from '@/sync/RecentEntries';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

type SetDraft = { reps: number; weight: number };
type Entry = { exercise: Exercise; sets: SetDraft[] };

const DEFAULT_SET: SetDraft = { reps: 10, weight: 20 };
const WEIGHT_STEP = 2.5;
const DURATION_STEP_MIN = 5;

function Stepper({
  label,
  value,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  unit: string;
  onChange: (delta: number) => void;
}) {
  const t = useTheme();
  const btn = [styles.stepBtn, { borderColor: t.border, backgroundColor: t.bg }];
  return (
    <View style={styles.stepper}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`Less ${label}`}
        onPress={() => onChange(-1)}
        style={btn}
      >
        <Text style={[styles.stepGlyph, { color: t.text }]}>−</Text>
      </Pressable>
      <View style={styles.stepValue}>
        <Text style={{ color: t.text, fontSize: fontSizes.cardTitle, fontWeight: '600' }}>
          {value}
        </Text>
        <Caption>{unit}</Caption>
      </View>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`More ${label}`}
        onPress={() => onChange(1)}
        style={btn}
      >
        <Text style={[styles.stepGlyph, { color: t.text }]}>+</Text>
      </Pressable>
    </View>
  );
}

function Chip({ label, onPress, active }: { label: string; onPress: () => void; active?: boolean }) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      onPress={onPress}
      style={[
        styles.chip,
        { borderColor: active ? t.strength : t.border, backgroundColor: t.card },
      ]}
    >
      <Text style={{ color: t.text, fontSize: fontSizes.body }}>{label}</Text>
    </Pressable>
  );
}

const round1 = (n: number) => Math.round(n * 10) / 10;

/** Shown after saving: rest advice, the protein tip and any new personal bests. */
function SavedPanel({ bests, onDone }: { bests: PersonalBest[]; onDone: () => void }) {
  const t = useTheme();
  return (
    <Card>
      <View style={{ gap: spacing(1.5) }}>
        <Text
          accessibilityRole="header"
          style={{ color: t.text, fontSize: fontSizes.cardTitle, fontWeight: '700' }}
        >
          {REST_MESSAGE}
        </Text>
        <Body muted>{PROTEIN_TIP}</Body>
        {bests.length > 0 ? (
          <View style={{ gap: spacing(0.5) }}>
            <Text style={{ color: t.accent, fontSize: fontSizes.body, fontWeight: '600' }}>
              New personal best
            </Text>
            {bests.map((b, i) => (
              <Caption key={i}>{bestLine(b)}</Caption>
            ))}
          </View>
        ) : null}
        <Button title="Done" onPress={onDone} />
      </View>
    </Card>
  );
}

export default function AddWorkout() {
  const t = useTheme();
  const router = useRouter();
  const queryClient = useQueryClient();
  // One id per form, so a retried Save can never create a second workout.
  const workoutId = useRef(newId());

  const [search, setSearch] = useState('');
  const [groupFilter, setGroupFilter] = useState<GroupFilter>('all');
  // null until the user edits; until then the program prefill (if any) is shown.
  const [edited, setEdited] = useState<Entry[] | null>(null);
  const [durationMin, setDurationMin] = useState(30);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [bests, setBests] = useState<PersonalBest[]>([]);
  const params = useLocalSearchParams<{ program?: string }>();

  const query = search.trim();
  const results = useQuery({
    queryKey: ['exercises', query],
    queryFn: () => searchExercises(query),
    placeholderData: keepPreviousData,
  });
  const recent = useQuery({ queryKey: ['workouts', 'recent'], queryFn: () => listWorkouts() });

  // Most recently used exercises and the last set of each, to prefill reps and weight.
  const { recentIds, lastSet } = useMemo(() => {
    const ids: string[] = [];
    const last = new Map<string, SetDraft>();
    for (const w of recent.data ?? []) {
      for (const s of w.sets) {
        if (!last.has(s.exercise_id)) {
          last.set(s.exercise_id, { reps: s.reps, weight: s.weight_kg });
          ids.push(s.exercise_id);
        }
      }
    }
    return { recentIds: ids.slice(0, 6), lastSet: last };
  }, [recent.data]);

  const catalog = useMemo(() => results.data ?? [], [results.data]);
  const recentExercises = useMemo(() => {
    const byId = new Map(catalog.map((e) => [e.id, e]));
    return recentIds.map((id) => byId.get(id)).filter((e): e is Exercise => !!e);
  }, [catalog, recentIds]);

  // "Start" on the program screen opens this form with today's session filled in.
  const prefill = useMemo<Entry[]>(() => {
    if (!params.program || !results.data) return [];
    const byId = new Map(results.data.map((e) => [e.id, e]));
    const byName = new Map(results.data.map((e) => [e.name.toLowerCase(), e]));
    const next: Entry[] = [];
    for (const p of parsePrefill(params.program)) {
      const exercise = (p.id ? byId.get(p.id) : undefined) ?? byName.get(p.name.toLowerCase());
      if (!exercise || next.some((n) => n.exercise.id === exercise.id)) continue;
      const kg = p.kg > 0 ? p.kg : (lastSet.get(exercise.id)?.weight ?? DEFAULT_SET.weight);
      next.push({
        exercise,
        sets: Array.from({ length: Math.max(1, Math.min(LIMITS.maxSets, p.sets)) }, () => ({
          reps: clamp(p.reps, LIMITS.reps.min, LIMITS.reps.max),
          weight: kg,
        })),
      });
    }
    return next;
  }, [params.program, results.data, lastSet]);
  const entries = edited ?? prefill;
  const setEntries = (update: Entry[] | ((prev: Entry[]) => Entry[])) =>
    setEdited((prev) => (typeof update === 'function' ? update(prev ?? prefill) : update));

  // After a save, look once for personal bests (the save itself is queued for sync).
  useEffect(() => {
    if (!saved) return;
    let live = true;
    const look = setTimeout(() => {
      getProgression()
        .then((p) => live && setBests(p.new_personal_bests ?? []))
        .catch(() => {});
    }, 2500);
    const leave = setTimeout(() => goBack(router), 8000);
    return () => {
      live = false;
      clearTimeout(look);
      clearTimeout(leave);
    };
  }, [saved, router]);

  const filters = useMemo(() => groupFilters(catalog), [catalog]);
  const sections = useMemo(
    () => buildSections(catalog, groupFilter, query),
    [catalog, groupFilter, query],
  );
  const pickedIds = useMemo(() => new Set(entries.map((e) => e.exercise.id)), [entries]);
  const exactMatch = catalog.some((e) => e.name.toLowerCase() === query.toLowerCase());

  function addExercise(exercise: Exercise) {
    setEntries((prev) =>
      prev.some((e) => e.exercise.id === exercise.id)
        ? prev
        : [...prev, { exercise, sets: [lastSet.get(exercise.id) ?? DEFAULT_SET] }],
    );
    setSearch('');
    setError(null);
  }

  const addCustom = useMutation({
    mutationFn: (group: MuscleGroup) => createExercise(query, group),
    onSuccess: (exercise) => {
      void queryClient.invalidateQueries({ queryKey: ['exercises'] });
      addExercise(exercise);
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : 'Could not add that exercise.'),
  });

  function changeSet(ei: number, si: number, patch: (s: SetDraft) => SetDraft) {
    setEntries((prev) =>
      prev.map((e, i) =>
        i === ei ? { ...e, sets: e.sets.map((s, j) => (j === si ? patch(s) : s)) } : e,
      ),
    );
  }

  function addSet(ei: number) {
    setEntries((prev) =>
      prev.map((e, i) =>
        i === ei && e.sets.length < LIMITS.maxSets
          ? { ...e, sets: [...e.sets, e.sets[e.sets.length - 1] ?? DEFAULT_SET] }
          : e,
      ),
    );
  }

  function removeSet(ei: number, si: number) {
    setEntries((prev) =>
      prev
        .map((e, i) => (i === ei ? { ...e, sets: e.sets.filter((_, j) => j !== si) } : e))
        .filter((e) => isTimeOnly(e.exercise.muscle_group) || e.sets.length > 0),
    );
  }

  const save = useMutation({
    mutationFn: () =>
      saveWorkout({
        id: workoutId.current,
        started_at: new Date().toISOString(),
        duration_s: clamp(durationMin * 60, LIMITS.duration_s.min, LIMITS.duration_s.max),
        // Cardio and focus have no sets to count; a single placeholder set keeps them in the log.
        sets: entries.flatMap((e) =>
          isTimeOnly(e.exercise.muscle_group)
            ? [{ exercise_id: e.exercise.id, reps: 1, weight_kg: 0 }]
            : e.sets.map((s) => ({
                exercise_id: e.exercise.id,
                reps: s.reps,
                weight_kg: s.weight,
              })),
        ),
      }),
    onSuccess: () => {
      refreshAfterSave(queryClient, ['workouts']);
      void queryClient.invalidateQueries({ queryKey: ['progression'] });
      setSaved(true);
    },
    onError: (e) =>
      setError(
        e instanceof ApiError ? e.message : 'Could not save your workout. Please try again.',
      ),
  });

  function onSave() {
    if (entries.length === 0) {
      setError('Pick at least one exercise.');
      return;
    }
    setError(null);
    save.mutate();
  }

  if (saved) {
    return (
      <Screen>
        <Title>Log workout</Title>
        <SavedPanel bests={bests} onDone={() => goBack(router)} />
      </Screen>
    );
  }

  return (
    <Screen>
      <ScrollView
        contentContainerStyle={{ gap: spacing(2), paddingVertical: spacing(2) }}
        keyboardShouldPersistTaps="handled"
      >
        <Title>Log workout</Title>

        <Field
          label="Exercise"
          placeholder="Search, for example bench press"
          value={search}
          onChangeText={setSearch}
          autoCorrect={false}
          returnKeyType="search"
        />

        {!query && recentExercises.length > 0 ? (
          <View style={{ gap: spacing(1) }}>
            <Caption>Recent</Caption>
            <View style={styles.wrap}>
              {recentExercises.map((e) => (
                <Chip key={e.id} label={e.name} onPress={() => addExercise(e)} />
              ))}
            </View>
          </View>
        ) : null}

        <ScrollView
          horizontal
          showsHorizontalScrollIndicator={false}
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={styles.filters}
        >
          {filters.map((g) => (
            <Chip
              key={g}
              label={g === 'all' ? 'All' : groupLabel(g)}
              active={g === groupFilter}
              onPress={() => setGroupFilter(g)}
            />
          ))}
        </ScrollView>

        {sections.length > 0 ? (
          <View style={[styles.picker, { borderColor: t.border, backgroundColor: t.card }]}>
            <SectionList
              sections={sections}
              keyExtractor={(e) => e.id}
              nestedScrollEnabled
              keyboardShouldPersistTaps="handled"
              stickySectionHeadersEnabled
              initialNumToRender={24}
              renderSectionHeader={({ section }) => (
                <View style={[styles.sectionHeader, { backgroundColor: t.card }]}>
                  <Caption>{section.title}</Caption>
                </View>
              )}
              renderItem={({ item }) => (
                <Pressable
                  accessibilityRole="button"
                  accessibilityLabel={`Add ${item.name}`}
                  onPress={() => addExercise(item)}
                  style={[styles.item, { borderColor: t.border }]}
                >
                  <Text style={{ color: t.text, fontSize: fontSizes.body }}>{item.name}</Text>
                  {pickedIds.has(item.id) ? <Caption>Added</Caption> : null}
                </Pressable>
              )}
            />
          </View>
        ) : results.isLoading ? (
          <Caption>Loading exercises...</Caption>
        ) : null}

        {query && !exactMatch && !results.isFetching ? (
          <View style={{ gap: spacing(1) }}>
            <Caption>{`Not found. Add “${query}” as your own, pick a type:`}</Caption>
            <View style={styles.wrap}>
              {MUSCLE_GROUPS.map((g) => (
                <Chip key={g} label={g} onPress={() => addCustom.mutate(g)} />
              ))}
            </View>
          </View>
        ) : null}

        <RecentEntries
          table="workouts"
          server={recent.data as unknown as { id: string; updated_at: string }[] | undefined}
          describe={(e) => {
            const when = new Date(String(e.data.started_at)).toLocaleDateString();
            const min = Math.round((Number(e.data.duration_s) || 0) / 60);
            return `Workout ${when} · ${min} min`;
          }}
        />

        {entries.map((entry, ei) => {
          const cardio = isTimeOnly(entry.exercise.muscle_group);
          return (
            <Card key={entry.exercise.id}>
              <View style={{ gap: spacing(1.5) }}>
                <View style={styles.row}>
                  <Text style={{ color: t.text, fontSize: fontSizes.cardTitle, fontWeight: '600' }}>
                    {entry.exercise.name}
                  </Text>
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel={`Remove ${entry.exercise.name}`}
                    onPress={() => setEntries((prev) => prev.filter((_, i) => i !== ei))}
                  >
                    <Caption>Remove</Caption>
                  </Pressable>
                </View>
                {cardio ? (
                  <Caption>Set the time below. No sets needed.</Caption>
                ) : (
                  <>
                    {entry.sets.map((s, si) => (
                      <View key={si} style={{ gap: spacing(1) }}>
                        <View style={styles.row}>
                          <Caption>{`Set ${si + 1}`}</Caption>
                          <Pressable
                            accessibilityRole="button"
                            accessibilityLabel={`Remove set ${si + 1}`}
                            onPress={() => removeSet(ei, si)}
                          >
                            <Caption>Remove</Caption>
                          </Pressable>
                        </View>
                        <View style={styles.row}>
                          <Stepper
                            label="reps"
                            unit="reps"
                            value={s.reps}
                            onChange={(d) =>
                              changeSet(ei, si, (x) => ({
                                ...x,
                                reps: clamp(x.reps + d, LIMITS.reps.min, LIMITS.reps.max),
                              }))
                            }
                          />
                          <Stepper
                            label="weight"
                            unit="kg"
                            value={s.weight}
                            onChange={(d) =>
                              changeSet(ei, si, (x) => ({
                                ...x,
                                weight: round1(
                                  clamp(
                                    x.weight + d * WEIGHT_STEP,
                                    LIMITS.weight_kg.min,
                                    LIMITS.weight_kg.max,
                                  ),
                                ),
                              }))
                            }
                          />
                        </View>
                      </View>
                    ))}
                    <Button title="Add set" variant="plain" onPress={() => addSet(ei)} />
                  </>
                )}
              </View>
            </Card>
          );
        })}

        <Card>
          <View style={styles.row}>
            <Body>How long?</Body>
            <Stepper
              label="minutes"
              unit="min"
              value={durationMin}
              onChange={(d) =>
                setDurationMin((m) =>
                  clamp(m + d * DURATION_STEP_MIN, 1, LIMITS.duration_s.max / 60),
                )
              }
            />
          </View>
        </Card>

        {error ? <Caption warn>{error}</Caption> : null}
        <Button title="Save workout" onPress={onSave} busy={save.isPending} />
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 8 },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  filters: { flexDirection: 'row', gap: 8 },
  picker: { height: 320, borderWidth: 1, borderRadius: radius, overflow: 'hidden' },
  sectionHeader: { paddingHorizontal: spacing(1.5), paddingVertical: spacing(1) },
  item: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 44,
    paddingHorizontal: spacing(1.5),
    borderTopWidth: StyleSheet.hairlineWidth,
  },
  chip: {
    borderWidth: 1,
    borderRadius: radius,
    paddingHorizontal: spacing(1.5),
    paddingVertical: spacing(1),
  },
  stepper: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  stepBtn: {
    width: 44,
    height: 44,
    borderRadius: radius,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  stepGlyph: { fontSize: 22, fontWeight: '500' },
  stepValue: { minWidth: 44, alignItems: 'center' },
});
