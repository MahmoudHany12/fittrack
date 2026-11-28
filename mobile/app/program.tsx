import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { View } from 'react-native';

import {
  getProgram,
  groupExercises,
  programKeys,
  restText,
  startParams,
  targetText,
} from '@/api/program';
import { Card } from '@/components/Card';
import { ScrollScreen } from '@/components/forms';
import { Body, Button, Caption, Title } from '@/components/ui';

export default function Program() {
  const router = useRouter();
  const q = useQuery({ queryKey: programKeys.today, queryFn: getProgram });
  const session = q.data;

  return (
    <ScrollScreen>
      <Title>{session?.title ?? "Today's session"}</Title>
      {q.isPending ? <Caption>Loading...</Caption> : null}
      {q.isError ? <Caption warn>Could not load your program.</Caption> : null}
      {session ? (
        <>
          <Caption>{`${session.name} · week ${session.week} of ${session.weeks} · day ${session.day}`}</Caption>
          {session.completed ? <Body muted>You finished this program. Nice work.</Body> : null}
          {groupExercises(session.exercises).map((g, gi) => (
            <Card key={gi}>
              <View style={{ gap: 12 }}>
                {g.superset ? <Caption>Superset: do these back to back</Caption> : null}
                {g.items.map((e) => (
                  <View key={e.name} style={{ gap: 4 }}>
                    <Body>{e.name}</Body>
                    <Caption>{`${e.sets} × ${e.reps} · ${targetText(e.weight_kg)}`}</Caption>
                    {e.note ? <Caption>{e.note}</Caption> : null}
                  </View>
                ))}
              </View>
            </Card>
          ))}
          {restText(session.rest) ? <Caption>{restText(session.rest)}</Caption> : null}
          {session.exercises.length > 0 ? (
            <Button
              title="Start"
              onPress={() => router.push({ pathname: '/add/workout', params: startParams(session) })}
            />
          ) : null}
        </>
      ) : null}
    </ScrollScreen>
  );
}
