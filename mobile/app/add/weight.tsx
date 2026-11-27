import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useRef, useState } from 'react';

import { getProfile, profileKeys } from '@/api/profile';
import { formatWeight, listWeight, newId, parseWeightInput, unitLabel, weightKeys } from '@/api/weight';
import { saveWeight } from '@/sync/store';
import { goBack, refreshAfterSave } from '@/sync/hooks';
import { RecentEntries } from '@/sync/RecentEntries';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';

export default function AddWeight() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const profile = useQuery({ queryKey: profileKeys.all, queryFn: getProfile });
  const units = profile.data?.units ?? 'metric';
  const recent = useQuery({ queryKey: [...weightKeys.all, 'recent'], queryFn: () => listWeight() });
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  // One id per screen visit so a retried save cannot create a duplicate.
  const id = useRef(newId());

  const save = useMutation({
    mutationFn: (kg: number) => saveWeight({ id: id.current, kg }),
    onSuccess: () => {
      refreshAfterSave(queryClient, weightKeys.all);
      goBack(router);
    },
    onError: () => setError('Could not save. Check your connection and try again.'),
  });

  const submit = () => {
    const kg = parseWeightInput(text, units);
    if (kg === null) {
      setError(`Enter a weight between 20 and 400 kg (${unitLabel(units)} shown).`);
      return;
    }
    setError(null);
    save.mutate(kg);
  };

  return (
    <Screen>
      <Title>Weight</Title>
      <Body muted>Log your weight for today.</Body>
      <Field
        label={`Weight (${unitLabel(units)})`}
        value={text}
        onChangeText={setText}
        keyboardType="decimal-pad"
        error={error ?? undefined}
      />
      <Button title="Save" onPress={submit} busy={save.isPending} />
      <RecentEntries
        table="weight"
        server={recent.data as unknown as { id: string; updated_at: string }[] | undefined}
        describe={(e) =>
          `${new Date(String(e.data.measured_at)).toLocaleDateString()} · ${formatWeight(Number(e.data.kg) || 0, units)}`
        }
      />
      {profile.isError ? <Caption>Using metric units; could not load your settings.</Caption> : null}
    </Screen>
  );
}
