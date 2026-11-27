import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useState } from 'react';
import { ScrollView, Share, Switch, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  acceptInvite,
  createInvite,
  friendKeys,
  inviteMessage,
  listFriends,
  parseInviteCode,
  setShare,
  type Friend,
} from '@/api/friends';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';
import { spacing, useTheme } from '@/theme';

function errorText(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

function FriendCard({ friend }: { friend: Friend }) {
  const t = useTheme();
  const router = useRouter();
  const queryClient = useQueryClient();
  const toggle = useMutation({
    mutationFn: (enabled: boolean) => setShare(friend.user_id, enabled),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: friendKeys.all }),
  });
  return (
    <Card>
      <View style={{ gap: spacing(1.5) }}>
        <View>
          <Body>{friend.display_name}</Body>
          <Caption>
            {friend.sharing_with_me
              ? 'Tap to see their weekly summary'
              : "They haven't shared with you yet"}
          </Caption>
        </View>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: spacing(1) }}>
          <View style={{ flex: 1 }}>
            <Body>Share my weekly summary</Body>
            <Caption>
              Only your weekly summary, nothing else. You can turn it off any time.
            </Caption>
          </View>
          <Switch
            accessibilityLabel={`Share my weekly summary with ${friend.display_name}`}
            value={friend.i_share}
            disabled={toggle.isPending}
            onValueChange={(v) => toggle.mutate(v)}
            trackColor={{ true: t.accent, false: t.border }}
          />
        </View>
        {toggle.isError ? <Caption warn>Could not change sharing. Try again.</Caption> : null}
        <Button
          title="View their summary"
          variant="plain"
          onPress={() => router.push({ pathname: '/friend/[id]', params: { id: friend.user_id } })}
        />
      </View>
    </Card>
  );
}

export default function Friends() {
  const queryClient = useQueryClient();
  const friends = useQuery({ queryKey: friendKeys.all, queryFn: listFriends });
  const [code, setCode] = useState('');
  const [codeError, setCodeError] = useState<string | null>(null);

  const invite = useMutation({
    mutationFn: async () => {
      const inv = await createInvite();
      await Share.share({ message: inviteMessage(inv) });
    },
  });

  const accept = useMutation({
    mutationFn: (c: string) => acceptInvite(c),
    onSuccess: async () => {
      setCode('');
      setCodeError(null);
      await queryClient.invalidateQueries({ queryKey: friendKeys.all });
    },
    onError: (e) => {
      if (e instanceof ApiError && e.status === 404) setCodeError('That code did not work. It may have expired or been used.');
      else if (e instanceof ApiError && e.status === 409) setCodeError('You are already friends.');
      else setCodeError(errorText(e, 'Could not add friend. Try again.'));
    },
  });

  const submitCode = () => {
    const parsed = parseInviteCode(code);
    if (!parsed) {
      setCodeError('Enter the 10-character code or the invite link.');
      return;
    }
    setCodeError(null);
    accept.mutate(parsed);
  };

  return (
    <Screen>
      <Title>Friends</Title>
      <ScrollView contentContainerStyle={{ gap: spacing(2), paddingBottom: spacing(4) }}>
        <Button
          title="Invite a friend"
          onPress={() => invite.mutate()}
          busy={invite.isPending}
        />
        {invite.isError ? <Caption warn>{errorText(invite.error, 'Could not create an invite.')}</Caption> : null}

        <Card>
          <View style={{ gap: spacing(1) }}>
            <Field
              label="Enter a code"
              value={code}
              onChangeText={setCode}
              autoCapitalize="characters"
              autoCorrect={false}
              placeholder="ABCDEFGHJK or invite link"
              error={codeError ?? undefined}
            />
            <Button title="Add friend" variant="plain" onPress={submitCode} busy={accept.isPending} />
          </View>
        </Card>

        {friends.isError ? <Caption warn>Could not load your friends.</Caption> : null}
        {friends.data && friends.data.length === 0 ? (
          <Body muted>No friends yet. Invite someone to compare progress.</Body>
        ) : null}
        {friends.data?.map((f) => <FriendCard key={f.user_id} friend={f} />)}
      </ScrollView>
    </Screen>
  );
}
