import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useState } from 'react';

import {
  ACTIVITY_LEVELS,
  type ActivityLevel,
  type CalorieMode,
  GOAL_RATES,
  type Goal,
  getProfile,
  profileKeys,
  type Profile,
  saveProfile,
  type Sex,
  validateBirthDate,
  validateCalorieTarget,
  validateHeight,
} from '@/api/profile';
import { targetsKeys } from '@/api/targets';
import { displayWeight, parseWeightInput, unitLabel, type Units } from '@/api/weight';
import { useAuth } from '@/auth/AuthProvider';
import { ScrollScreen, Segmented, SectionTitle } from '@/components/forms';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';

export default function Settings() {
  const { signOut } = useAuth();
  const profile = useQuery({ queryKey: profileKeys.all, queryFn: getProfile });

  if (!profile.data) {
    return (
      <Screen>
        <Title>Settings</Title>
        {profile.isError ? <Caption warn>Could not load your profile.</Caption> : <Caption>Loading...</Caption>}
        <Button title="Sign out" variant="plain" onPress={() => void signOut()} />
      </Screen>
    );
  }
  return <SettingsForm profile={profile.data} signOut={signOut} />;
}

function SettingsForm({ profile, signOut }: { profile: Profile; signOut: () => Promise<void> | void }) {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [name, setName] = useState(profile.display_name ?? '');
  const [calories, setCalories] = useState(String(profile.calorie_target));
  const [goal, setGoal] = useState(
    profile.weight_goal_kg === null ? '' : displayWeight(profile.weight_goal_kg, profile.units),
  );
  const [units, setUnits] = useState<Units>(profile.units);
  const [height, setHeight] = useState(profile.height_cm == null ? '' : String(profile.height_cm));
  const [birth, setBirth] = useState(profile.birth_date ?? '');
  const [sex, setSex] = useState<Sex | null>(profile.sex ?? null);
  const [activity, setActivity] = useState<ActivityLevel | null>(profile.activity_level ?? null);
  const [bodyGoal, setBodyGoal] = useState<Goal | null>(profile.goal ?? null);
  const [rate, setRate] = useState<number>(profile.goal_rate_kg_per_week ?? 0.5);
  const [mode, setMode] = useState<CalorieMode>(profile.calorie_target_mode ?? 'auto');
  const [errors, setErrors] = useState<{
    calories?: string;
    goal?: string;
    height?: string;
    birth?: string;
    save?: string;
  }>({});
  const [saved, setSaved] = useState(false);

  const save = useMutation({
    mutationFn: saveProfile,
    onSuccess: (p) => {
      queryClient.setQueryData(profileKeys.all, p);
      void queryClient.invalidateQueries({ queryKey: targetsKeys.all });
      void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      setSaved(true);
    },
    onError: () => setErrors({ save: 'Could not save. Try again.' }),
  });

  const switchUnits = (next: Units) => {
    // Re-express a valid goal in the new units so it stays the same weight.
    const kg = goal.trim() ? parseWeightInput(goal, units) : null;
    if (kg !== null) setGoal(displayWeight(kg, next));
    setUnits(next);
  };

  const submit = () => {
    const next: typeof errors = {};
    const calErr = mode === 'manual' ? validateCalorieTarget(calories) : null;
    if (calErr) next.calories = calErr;
    const heightErr = validateHeight(height);
    if (heightErr) next.height = heightErr;
    const birthErr = validateBirthDate(birth);
    if (birthErr) next.birth = birthErr;
    const goalKg = goal.trim() ? parseWeightInput(goal, units) : null;
    if (goal.trim() && goalKg === null) next.goal = 'Enter a realistic goal weight.';
    setErrors(next);
    setSaved(false);
    if (next.calories || next.goal || next.height || next.birth) return;
    save.mutate({
      display_name: name.trim() || null,
      calorie_target: mode === 'manual' ? Number(calories.trim()) : profile.calorie_target,
      weight_goal_kg: goalKg,
      units,
      height_cm: height.trim() ? Number(height.trim().replace(',', '.')) : undefined,
      birth_date: birth.trim() || undefined,
      sex: sex ?? undefined,
      activity_level: activity ?? undefined,
      goal: bodyGoal ?? undefined,
      goal_rate_kg_per_week: bodyGoal && bodyGoal !== 'maintain' ? rate : undefined,
      calorie_target_mode: mode,
    });
  };

  const activityHint = ACTIVITY_LEVELS.find((a) => a.value === activity)?.hint;

  return (
    <ScrollScreen>
      <Title>Settings</Title>
      <Field label="Display name" value={name} onChangeText={setName} maxLength={50} />
      <Field
        label={`Goal weight (${unitLabel(units)})`}
        value={goal}
        onChangeText={setGoal}
        keyboardType="decimal-pad"
        error={errors.goal}
      />
      <Body muted>Units</Body>
      <Button
        title={units === 'metric' ? 'Metric (kg) - tap for imperial' : 'Imperial (lb) - tap for metric'}
        variant="plain"
        onPress={() => switchUnits(units === 'metric' ? 'imperial' : 'metric')}
      />
      <SectionTitle>Body profile</SectionTitle>
      <Field
        label="Height (cm)"
        value={height}
        onChangeText={setHeight}
        keyboardType="decimal-pad"
        error={errors.height}
      />
      <Field
        label="Birth date (YYYY-MM-DD)"
        value={birth}
        onChangeText={setBirth}
        autoCapitalize="none"
        error={errors.birth}
      />
      <Segmented
        label="Sex"
        options={[
          { value: 'male', label: 'Male' },
          { value: 'female', label: 'Female' },
        ]}
        value={sex}
        onChange={setSex}
      />
      <Segmented
        label="Activity level"
        options={ACTIVITY_LEVELS.map((a) => ({ value: a.value, label: a.label }))}
        value={activity}
        onChange={setActivity}
      />
      {activityHint ? <Caption>{activityHint}</Caption> : null}
      <Segmented
        label="Goal"
        options={[
          { value: 'lose', label: 'Lose' },
          { value: 'maintain', label: 'Maintain' },
          { value: 'gain', label: 'Gain' },
        ]}
        value={bodyGoal}
        onChange={setBodyGoal}
      />
      {bodyGoal && bodyGoal !== 'maintain' ? (
        <Segmented
          label="Weekly rate (kg per week)"
          options={GOAL_RATES.map((r) => ({ value: r, label: String(r) }))}
          value={rate}
          onChange={setRate}
        />
      ) : null}
      <Segmented
        label="Calorie target"
        options={[
          { value: 'auto', label: 'Auto from my body' },
          { value: 'manual', label: 'Manual' },
        ]}
        value={mode}
        onChange={setMode}
      />
      {mode === 'manual' ? (
        <Field
          label="Daily calorie target"
          value={calories}
          onChangeText={setCalories}
          keyboardType="number-pad"
          error={errors.calories}
        />
      ) : null}
      <Button title="Save" onPress={submit} busy={save.isPending} />
      {saved ? <Caption>Saved.</Caption> : null}
      {errors.save ? <Caption warn>{errors.save}</Caption> : null}
      <Button title="Daily targets" variant="plain" onPress={() => router.push('/targets')} />
      <Button title="Health data import" variant="plain" onPress={() => router.push('/health-permissions')} />
      <Button title="Weekly summary and export" variant="plain" onPress={() => router.push('/summary')} />
      <Button title="Sign out" variant="plain" onPress={() => void signOut()} />
    </ScrollScreen>
  );
}
