import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { View } from 'react-native';

import { getProgram, programKeys, restText, startParams, targetText } from '@/api/program';
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
      {q.isSuccess && !session ? (
        <Body muted>Nothing planned for today. Rest and recover.</Body>
      ) : null}
      {session ? (
        <>
          {session.program ? (
            <Caption>
              {[
                session.program,
                session.week ? `week ${session.week}` : null,
                session.day ? `day ${session.day}` : null,
              ]
                .filter(Boolean)
                .join(' · ')}
            </Caption>
          ) : null}
          {session.exercises.map((e) => (
            <Card key={e.exercise_id}>
              <View style={{ gap: 4 }}>
                <Body>{e.name}</Body>
                <Caption>{`${e.sets} × ${e.reps} · ${targetText(e.target_weight_kg)}`}</Caption>
                {restText(e.rest_s) ? <Caption>{restText(e.rest_s)}</Caption> : null}
              </View>
            </Card>
          ))}
          <Button
            title="Start"
            onPress={() => router.push({ pathname: '/add/workout', params: startParams(session) })}
          />
        </>
      ) : null}
    </ScrollScreen>
  );
}
