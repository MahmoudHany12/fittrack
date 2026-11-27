import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useMemo, useRef, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  LIMITS,
  MUSCLE_GROUPS,
  clamp,
  createExercise,
  listWorkouts,
  newId,
  searchExercises,
  type Exercise,
  type MuscleGroup,
} from '@/api/workouts';
import { saveWorkout } from '@/sync/store';
import { LOCAL_KEY } from '@/sync/hooks';
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

export default function AddWorkout() {
  const t = useTheme();
  const router = useRouter();
  const queryClient = useQueryClient();
  // One id per form, so a retried Save can never create a second workout.
  const workoutId = useRef(newId());

  const [search, setSearch] = useState('');
  const [entries, setEntries] = useState<Entry[]>([]);
  const [durationMin, setDurationMin] = useState(30);
  const [error, setError] = useState<string | null>(null);

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

  const catalog = results.data ?? [];
  const recentExercises = useMemo(() => {
    const byId = new Map(catalog.map((e) => [e.id, e]));
    return recentIds.map((id) => byId.get(id)).filter((e): e is Exercise => !!e);
  }, [catalog, recentIds]);

  const shown = query ? catalog.slice(0, 6) : [];
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
        .filter((e) => e.exercise.muscle_group === 'cardio' || e.sets.length > 0),
    );
  }

  const save = useMutation({
    mutationFn: () =>
      saveWorkout({
        id: workoutId.current,
        started_at: new Date().toISOString(),
        duration_s: clamp(durationMin * 60, LIMITS.duration_s.min, LIMITS.duration_s.max),
        // Cardio has no sets to count; a single placeholder set keeps it in the log.
        sets: entries.flatMap((e) =>
          e.exercise.muscle_group === 'cardio'
            ? [{ exercise_id: e.exercise.id, reps: 1, weight_kg: 0 }]
            : e.sets.map((s) => ({
                exercise_id: e.exercise.id,
                reps: s.reps,
                weight_kg: s.weight,
              })),
        ),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['workouts'] });
      void queryClient.invalidateQueries({ queryKey: LOCAL_KEY });
      router.back();
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

        {shown.length > 0 ? (
          <View style={styles.wrap}>
            {shown.map((e) => (
              <Chip key={e.id} label={e.name} onPress={() => addExercise(e)} />
            ))}
          </View>
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
          const cardio = entry.exercise.muscle_group === 'cardio';
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
