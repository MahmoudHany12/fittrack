import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { ScrollView, View } from 'react-native';

import { ApiError } from '@/api/client';
import { friendKeys, getFriendSummary, removeFriend, summaryRows } from '@/api/friends';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Screen, Title } from '@/components/ui';
import { spacing } from '@/theme';

export default function FriendSummaryScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const queryClient = useQueryClient();

  const summary = useQuery({
    queryKey: friendKeys.summary(id),
    queryFn: () => getFriendSummary(id),
    enabled: Boolean(id),
    retry: false,
  });

  const remove = useMutation({
    mutationFn: () => removeFriend(id),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: friendKeys.all });
      router.back();
    },
  });

  const forbidden = summary.error instanceof ApiError && summary.error.status === 403;
  const gone = summary.error instanceof ApiError && summary.error.status === 404;
  const rows = summary.data ? summaryRows(summary.data.summary) : [];

  return (
    <Screen>
      <Title>{summary.data?.display_name ?? 'Friend'}</Title>
      <ScrollView contentContainerStyle={{ gap: spacing(2), paddingBottom: spacing(4) }}>
        {summary.isPending && id ? <Caption>Loading...</Caption> : null}
        {forbidden ? <Body muted>They haven&apos;t shared with you yet.</Body> : null}
        {gone ? <Body muted>This person is no longer your friend.</Body> : null}
        {summary.isError && !forbidden && !gone ? (
          <Caption warn>Could not load the summary.</Caption>
        ) : null}
        {summary.data ? (
          <Card>
            <View style={{ gap: spacing(1) }}>
              <Caption>Weekly summary</Caption>
              {rows.length === 0 ? (
                <Body muted>Nothing to show yet this week.</Body>
              ) : (
                rows.map((r) => (
                  <View key={r.label} style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
                    <Body muted>{r.label}</Body>
                    <Body>{r.value}</Body>
                  </View>
                ))
              )}
            </View>
          </Card>
        ) : null}
        {!gone ? (
          <Button
            title="Remove friend"
            variant="plain"
            onPress={() => remove.mutate()}
            busy={remove.isPending}
          />
        ) : null}
        {remove.isError ? <Caption warn>Could not remove friend. Try again.</Caption> : null}
        <Caption>Removing a friend stops all sharing in both directions.</Caption>
      </ScrollView>
    </Screen>
  );
}
