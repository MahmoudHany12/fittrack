import { useQuery } from '@tanstack/react-query';
import { View } from 'react-native';

import { dateText, getRecords, kgText, liftText, recordsKeys } from '@/api/records';
import { Card } from '@/components/Card';
import { ScrollScreen } from '@/components/forms';
import { Body, Caption, Title } from '@/components/ui';

export default function Records() {
  const q = useQuery({ queryKey: recordsKeys.all, queryFn: getRecords });
  const list = q.data ?? [];

  return (
    <ScrollScreen>
      <Title>Personal bests</Title>
      {q.isPending ? <Caption>Loading...</Caption> : null}
      {q.isError ? <Caption warn>Could not load your records.</Caption> : null}
      {q.isSuccess && list.length === 0 ? (
        <Body muted>No records yet. Log a workout and your best lifts show up here.</Body>
      ) : null}
      {list.map((r) => (
        <Card key={r.exercise_id}>
          <View style={{ gap: 4 }}>
            <Body>{r.exercise_name}</Body>
            <Caption>{`Heaviest ${liftText(r.best_weight_kg, r.best_weight_reps)}`}</Caption>
            <Caption>{`Estimated 1RM ${kgText(r.best_e1rm_kg)}`}</Caption>
            {dateText(r.best_weight_at) ? <Caption>{dateText(r.best_weight_at)}</Caption> : null}
          </View>
        </Card>
      ))}
    </ScrollScreen>
  );
}
