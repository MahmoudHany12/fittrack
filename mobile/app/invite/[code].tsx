import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { View } from 'react-native';

import { ApiError } from '@/api/client';
import { acceptInvite, friendKeys, parseInviteCode } from '@/api/friends';
import { Body, Button, Caption, Screen, Title } from '@/components/ui';
import { spacing } from '@/theme';

/** Plain-language text for a failed accept. */
export function inviteErrorText(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.status === 404) return 'That invite has expired or was already used.';
    if (e.status === 409) return 'You are already friends.';
    if (e.status === 400) return "That's your own invite.";
  }
  return 'Could not accept the invite. Try again.';
}

/** Opened from a fittrack://invite/<code> link. */
export default function InviteScreen() {
  const { code: rawCode } = useLocalSearchParams<{ code: string | string[] }>();
  const router = useRouter();
  const queryClient = useQueryClient();
  const raw = Array.isArray(rawCode) ? rawCode[0] : rawCode;
  const code = parseInviteCode(raw ?? '');

  const accept = useMutation({
    mutationFn: (c: string) => acceptInvite(c),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: friendKeys.all });
      router.replace('/(tabs)/friends');
    },
  });

  const notNow = () => router.replace('/(tabs)/friends');

  if (!code) {
    return (
      <Screen>
        <Title>Invite not valid</Title>
        <Body muted>This invite link does not look right.</Body>
        <Button title="Back to friends" variant="plain" onPress={notNow} />
      </Screen>
    );
  }

  return (
    <Screen>
      <Title>Add {code} as a friend?</Title>
      <View style={{ gap: spacing(1.5) }}>
        {accept.isError ? <Caption warn>{inviteErrorText(accept.error)}</Caption> : null}
        <Button title="Accept invite" onPress={() => accept.mutate(code)} busy={accept.isPending} />
        <Button title="Not now" variant="plain" onPress={notNow} />
      </View>
    </Screen>
  );
}
