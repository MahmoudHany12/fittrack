import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { getProfile, profileKeys, saveProfile, validateCalorieTarget } from '@/api/profile';
import { displayWeight, parseWeightInput, unitLabel, type Units } from '@/api/weight';
import { useAuth } from '@/auth/AuthProvider';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';

export default function Settings() {
  const { signOut } = useAuth();
  const queryClient = useQueryClient();
  const profile = useQuery({ queryKey: profileKeys.all, queryFn: getProfile });

  const [name, setName] = useState('');
  const [calories, setCalories] = useState('');
  const [goal, setGoal] = useState('');
  const [units, setUnits] = useState<Units>('metric');
  const [errors, setErrors] = useState<{ calories?: string; goal?: string; save?: string }>({});
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    const p = profile.data;
    if (!p) return;
    setName(p.display_name ?? '');
    setCalories(String(p.calorie_target));
    setUnits(p.units);
    setGoal(p.weight_goal_kg === null ? '' : displayWeight(p.weight_goal_kg, p.units));
  }, [profile.data]);

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
      {profile.isError ? <Caption warn>Could not load your profile.</Caption> : null}
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
