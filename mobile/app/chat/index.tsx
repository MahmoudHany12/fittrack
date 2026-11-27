import { useQuery } from '@tanstack/react-query';
import { useFocusEffect, useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { FlatList, Pressable, View } from 'react-native';

import { chatKeys, listConversations, POLL_MS, type Conversation } from '@/api/chat';
import { Body, Caption, Screen, Title } from '@/components/ui';
import { radius, spacing, useTheme } from '@/theme';

export default function ChatListScreen() {
  const t = useTheme();
  const router = useRouter();
  const [focused, setFocused] = useState(true);
  useFocusEffect(
    useCallback(() => {
      setFocused(true);
      return () => setFocused(false);
    }, []),
  );

  const list = useQuery({
    queryKey: chatKeys.list,
    queryFn: listConversations,
    refetchInterval: focused ? POLL_MS : false,
  });

  const open = (c: Conversation) => router.push({ pathname: '/chat/[id]', params: { id: c.friend_id } });

  return (
    <Screen>
      <Title>Chat</Title>
      {list.isPending ? <Caption>Loading...</Caption> : null}
      {list.isError ? <Caption warn>Could not load conversations.</Caption> : null}
      {list.data && list.data.length === 0 ? (
        <Body muted>No conversations yet. Open a friend to say hi.</Body>
      ) : null}
      <FlatList
        data={list.data ?? []}
        keyExtractor={(c) => c.friend_id}
        ItemSeparatorComponent={() => <View style={{ height: spacing(1) }} />}
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Chat with ${item.display_name}`}
            onPress={() => open(item)}
            style={{
              backgroundColor: t.card,
              borderColor: t.border,
              borderWidth: 1,
              borderRadius: radius,
              padding: spacing(1.5),
              flexDirection: 'row',
              alignItems: 'center',
              gap: spacing(1),
            }}
          >
            <View style={{ flex: 1, gap: spacing(0.5) }}>
              <Body>{item.display_name}</Body>
              <Caption>
                {item.last_from_me ? 'You: ' : ''}
                {item.last_message}
              </Caption>
            </View>
            {item.unread_count > 0 ? (
              <View
                accessibilityLabel={`${item.unread_count} unread`}
                style={{
                  backgroundColor: t.accent,
                  borderRadius: radius,
                  minWidth: 24,
                  paddingHorizontal: spacing(1),
                  paddingVertical: spacing(0.25),
                  alignItems: 'center',
                }}
              >
                <Caption>{String(item.unread_count)}</Caption>
              </View>
            ) : null}
          </Pressable>
        )}
      />
    </Screen>
  );
}
