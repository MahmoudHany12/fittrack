import { authedRequest } from './client';

export type Conversation = {
  friend_id: string;
  display_name: string;
  last_message: string;
  last_sent_at: string;
  last_from_me: boolean;
  unread_count: number;
};

export type ChatMessage = {
  id: string;
  from_user: string;
  to_user: string;
  mine: boolean;
  body: string;
  sent_at: string;
  read_at: string | null;
};

export const MAX_BODY = 2000;
export const POLL_MS = 5000;

export const chatKeys = {
  all: ['chat'] as const,
  list: ['chat', 'list'] as const,
  thread: (friendId: string) => ['chat', 'thread', friendId] as const,
};

export function listConversations(): Promise<Conversation[]> {
  return authedRequest<Conversation[]>('/chat');
}

/** Newest first. Fetching marks the caller's received messages as read. */
export function getThread(
  friendId: string,
  opts: { before?: string; limit?: number } = {},
): Promise<ChatMessage[]> {
  const params: string[] = [];
  if (opts.before) params.push(`before=${encodeURIComponent(opts.before)}`);
  if (opts.limit !== undefined) params.push(`limit=${opts.limit}`);
  const qs = params.length ? `?${params.join('&')}` : '';
  return authedRequest<ChatMessage[]>(`/chat/${encodeURIComponent(friendId)}${qs}`);
}

export function sendMessage(friendId: string, body: string): Promise<ChatMessage> {
  return authedRequest<ChatMessage>(`/chat/${encodeURIComponent(friendId)}`, 'POST', { body });
}

export function deleteMessage(friendId: string, id: string): Promise<void> {
  return authedRequest<void>(
    `/chat/${encodeURIComponent(friendId)}/messages/${encodeURIComponent(id)}`,
    'DELETE',
  );
}

/** Trim and check a draft the same way the server does; null means it cannot be sent. */
export function cleanDraft(raw: string): string | null {
  const body = raw.trim();
  if (body.length === 0 || body.length > MAX_BODY) return null;
  if (/[\u0000-\u0009\u000B-\u001F\u007F-\u009F]/.test(body)) return null;
  return body;
}

/** A placeholder shown immediately while the real send is in flight. */
export function optimisticMessage(friendId: string, body: string, now = new Date()): ChatMessage {
  return {
    id: `pending-${now.getTime()}`,
    from_user: '',
    to_user: friendId,
    mine: true,
    body,
    sent_at: now.toISOString(),
    read_at: null,
  };
}
