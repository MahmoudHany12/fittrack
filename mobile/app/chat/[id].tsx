import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, KeyboardAvoidingView, Platform, Pressable, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  chatKeys,
  cleanDraft,
  deleteMessage,
  getThread,
  MAX_BODY,
  optimisticMessage,
  POLL_MS,
  sendMessage,
  type ChatMessage,
} from '@/api/chat';
import { Button, Caption, Field, Screen, Title } from '@/components/ui';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

export default function ChatThreadScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const t = useTheme();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState('');
  const [focused, setFocused] = useState(true);
  useFocusEffect(
    useCallback(() => {
      setFocused(true);
      return () => setFocused(false);
    }, []),
  );

  const key = chatKeys.thread(id);
  const thread = useQuery({
    queryKey: key,
    queryFn: () => getThread(id),
    enabled: Boolean(id),
    retry: false,
    refetchInterval: focused ? POLL_MS : false,
  });

  const send = useMutation({
    mutationFn: (body: string) => sendMessage(id, body),
    onMutate: async (body) => {
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<ChatMessage[]>(key);
      queryClient.setQueryData<ChatMessage[]>(key, (old) => [
        optimisticMessage(id, body),
        ...(old ?? []),
      ]);
      return { previous };
    },
    onError: (_e, body, ctx) => {
      queryClient.setQueryData(key, ctx?.previous);
      setDraft((d) => d || body);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: key });
      void queryClient.invalidateQueries({ queryKey: chatKeys.list });
    },
  });

  const remove = useMutation({
    mutationFn: (messageId: string) => deleteMessage(id, messageId),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: key });
      void queryClient.invalidateQueries({ queryKey: chatKeys.list });
    },
  });

  const gone = thread.error instanceof ApiError && thread.error.status === 404;
  const clean = cleanDraft(draft);

  const submit = () => {
    if (!clean || send.isPending) return;
    setDraft('');
    send.mutate(clean);
  };

  return (
    <Screen>
      <KeyboardAvoidingView
        style={{ flex: 1, gap: spacing(2) }}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        <Title>Chat</Title>
        {thread.isPending && id ? <Caption>Loading...</Caption> : null}
        {gone ? <Caption>This conversation is not available.</Caption> : null}
        {thread.isError && !gone ? <Caption warn>Could not load messages.</Caption> : null}
        <FlatList
          inverted
          style={{ flex: 1 }}
          data={thread.data ?? []}
          keyExtractor={(m) => m.id}
          ItemSeparatorComponent={() => <View style={{ height: spacing(1) }} />}
          renderItem={({ item }) => (
            <Pressable
              accessibilityRole="button"
              accessibilityHint={item.mine ? 'Long press to delete' : undefined}
              onLongPress={() => {
                if (item.mine && !item.id.startsWith('pending-')) remove.mutate(item.id);
              }}
              style={{
                alignSelf: item.mine ? 'flex-end' : 'flex-start',
                maxWidth: '80%',
                backgroundColor: item.mine ? t.accent : t.card,
                borderColor: t.border,
                borderWidth: item.mine ? 0 : 1,
                borderRadius: radius,
                paddingHorizontal: spacing(1.5),
                paddingVertical: spacing(1),
              }}
            >
              <Text style={{ fontSize: fontSizes.body, color: item.mine ? '#0E1110' : t.text }}>
                {item.body}
              </Text>
            </Pressable>
          )}
        />
        {send.isError ? (
          <Caption warn>
            {send.error instanceof ApiError ? send.error.message : 'Could not send. Try again.'}
          </Caption>
        ) : null}
        {remove.isError ? <Caption warn>Could not delete that message.</Caption> : null}
        {!gone ? (
          <View style={{ gap: spacing(1) }}>
            <Field
              label="Message"
              value={draft}
              onChangeText={setDraft}
              multiline
              maxLength={MAX_BODY}
              placeholder="Write a message"
            />
            <Button title="Send" onPress={submit} busy={send.isPending} />
          </View>
        ) : null}
      </KeyboardAvoidingView>
    </Screen>
  );
}
