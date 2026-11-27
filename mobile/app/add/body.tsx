import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useRef, useState } from 'react';
import { View } from 'react-native';

import {
  bodyKeys,
  buildBodyInput,
  BODY_RANGES,
  listBody,
  saveBody,
  validateBodyForm,
  type BodyFormErrors,
  type BodyField,
  type BodyForm,
} from '@/api/body';
import { targetsKeys } from '@/api/targets';
import { newId, weightKeys } from '@/api/weight';
import { Card } from '@/components/Card';
import { ScrollScreen, SectionTitle } from '@/components/forms';
import { Body, Button, Caption, Field, Title } from '@/components/ui';

const FIELDS: { key: BodyField; label: string; hint?: string }[] = [
  { key: 'weight_kg', label: 'Weight (kg)' },
  { key: 'body_fat_pct', label: 'Body fat (%) - optional' },
  { key: 'skeletal_muscle_kg', label: 'Skeletal muscle mass (kg) - optional' },
  { key: 'fat_mass_kg', label: 'Fat mass (kg) - optional' },
  { key: 'visceral_fat_level', label: 'Visceral fat level - optional' },
  { key: 'bmr_kcal', label: 'BMR (kcal) - optional' },
  { key: 'body_water_pct', label: 'Body water (%) - optional' },
];

function emptyForm(): BodyForm {
  return {
    date: new Date().toISOString().slice(0, 10),
    weight_kg: '',
    body_fat_pct: '',
    skeletal_muscle_kg: '',
    fat_mass_kg: '',
    visceral_fat_level: '',
    bmr_kcal: '',
    body_water_pct: '',
  };
}

export default function AddBody() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const recent = useQuery({ queryKey: [...bodyKeys.all, 'recent'], queryFn: () => listBody() });
  const [form, setForm] = useState<BodyForm>(emptyForm);
  const [errors, setErrors] = useState<BodyFormErrors>({});
  const [saveError, setSaveError] = useState<string | null>(null);
  // One id per screen visit so a retried save cannot create a duplicate.
  const id = useRef(newId());

  const save = useMutation({
    mutationFn: () => saveBody(buildBodyInput(form, id.current)),
    onSuccess: async () => {
      await Promise.all(
        [['dashboard'], weightKeys.all, targetsKeys.all, bodyKeys.all].map((queryKey) =>
          queryClient.invalidateQueries({ queryKey }),
        ),
      );
      router.back();
    },
    onError: () => setSaveError('Could not save. Check your connection and try again.'),
  });

  const set = (k: keyof BodyForm) => (v: string) => setForm((f) => ({ ...f, [k]: v }));

  const submit = () => {
    const next = validateBodyForm(form);
    setErrors(next);
    setSaveError(null);
    if (Object.keys(next).length > 0) return;
    save.mutate();
  };

  const last = (recent.data ?? []).slice(0, 3);

  return (
    <ScrollScreen>
      <Title>Body scan</Title>
      <Body muted>Enter your InBody result. Only weight is required.</Body>
      <Field
        label="Date (YYYY-MM-DD)"
        value={form.date}
        onChangeText={set('date')}
        autoCapitalize="none"
        error={errors.date}
      />
      {FIELDS.map((f) => (
        <Field
          key={f.key}
          label={f.label}
          value={form[f.key]}
          onChangeText={set(f.key)}
          keyboardType="decimal-pad"
          placeholder={`${BODY_RANGES[f.key].min} to ${BODY_RANGES[f.key].max}`}
          error={errors[f.key]}
        />
      ))}
      <Button title="Save" onPress={submit} busy={save.isPending} />
      {saveError ? <Caption warn>{saveError}</Caption> : null}
      {last.length > 0 ? (
        <View style={{ gap: 8 }}>
          <SectionTitle>Recent scans</SectionTitle>
          {last.map((s) => (
            <Card key={s.id}>
              <Body>{new Date(s.measured_at).toLocaleDateString()}</Body>
              <Caption>
                {`${s.weight_kg} kg  |  fat ${s.body_fat_pct ?? '-'}%  |  muscle ${s.skeletal_muscle_kg ?? '-'} kg`}
              </Caption>
            </Card>
          ))}
        </View>
      ) : null}
    </ScrollScreen>
  );
}
