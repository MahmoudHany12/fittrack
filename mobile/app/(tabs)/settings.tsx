import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';

import { getProfile, profileKeys, type Profile, saveProfile, validateCalorieTarget } from '@/api/profile';
import { displayWeight, parseWeightInput, unitLabel, type Units } from '@/api/weight';
import { useAuth } from '@/auth/AuthProvider';
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
  const queryClient = useQueryClient();
  const [name, setName] = useState(profile.display_name ?? '');
  const [calories, setCalories] = useState(String(profile.calorie_target));
  const [goal, setGoal] = useState(
    profile.weight_goal_kg === null ? '' : displayWeight(profile.weight_goal_kg, profile.units),
  );
  const [units, setUnits] = useState<Units>(profile.units);
  const [errors, setErrors] = useState<{ calories?: string; goal?: string; save?: string }>({});
  const [saved, setSaved] = useState(false);

  const save = useMutation({
    mutationFn: saveProfile,
    onSuccess: (p) => {
      queryClient.setQueryData(profileKeys.all, p);
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
    const calErr = validateCalorieTarget(calories);
    if (calErr) next.calories = calErr;
    const goalKg = goal.trim() ? parseWeightInput(goal, units) : null;
    if (goal.trim() && goalKg === null) next.goal = 'Enter a realistic goal weight.';
    setErrors(next);
    setSaved(false);
    if (next.calories || next.goal) return;
    save.mutate({
      display_name: name.trim() || null,
      calorie_target: Number(calories.trim()),
      weight_goal_kg: goalKg,
      units,
    });
  };

  return (
    <Screen>
      <Title>Settings</Title>
      <Field label="Display name" value={name} onChangeText={setName} maxLength={50} />
      <Field
        label="Daily calorie target"
        value={calories}
        onChangeText={setCalories}
        keyboardType="number-pad"
        error={errors.calories}
      />
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
      <Button title="Save" onPress={submit} busy={save.isPending} />
      {saved ? <Caption>Saved.</Caption> : null}
      {errors.save ? <Caption warn>{errors.save}</Caption> : null}
      <Button title="Sign out" variant="plain" onPress={() => void signOut()} />
    </Screen>
  );
}
