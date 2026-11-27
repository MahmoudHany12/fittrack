import { authedRequest } from './client';

export type Friend = {
  user_id: string;
  display_name: string;
  /** This friend shares their weekly summary with me. */
  sharing_with_me: boolean;
  /** I share my weekly summary with this friend. */
  i_share: boolean;
};

export type Invite = { code: string; link: string; expires_at: string };

/** Shape is owned by the summary module; treated as opaque key/value data here. */
export type FriendSummary = {
  user_id: string;
  display_name: string;
  summary: Record<string, unknown>;
};

export const friendKeys = {
  all: ['friends'] as const,
  summary: (id: string) => ['friends', 'summary', id] as const,
};

export const CODE_LENGTH = 10;
const CODE_RE = /^[ABCDEFGHJKLMNPQRSTUVWXYZ23456789]{10}$/;

export function listFriends(): Promise<Friend[]> {
  return authedRequest<Friend[]>('/friends');
}

export function createInvite(): Promise<Invite> {
  return authedRequest<Invite>('/friends/invites', 'POST');
}

export function acceptInvite(code: string): Promise<{ friend_id: string }> {
  return authedRequest<{ friend_id: string }>(
    `/friends/invites/${encodeURIComponent(code)}/accept`,
    'POST',
  );
}

export function removeFriend(userId: string): Promise<void> {
  return authedRequest<void>(`/friends/${encodeURIComponent(userId)}`, 'DELETE');
}

/** Consent to (or revoke) sharing my weekly summary, and nothing else, with a friend. */
export function setShare(userId: string, enabled: boolean): Promise<{ enabled: boolean }> {
  return authedRequest<{ enabled: boolean }>(`/friends/${encodeURIComponent(userId)}/share`, 'PUT', {
    enabled,
  });
}

export function getFriendSummary(userId: string): Promise<FriendSummary> {
  return authedRequest<FriendSummary>(`/friends/${encodeURIComponent(userId)}/summary`);
}

/**
 * Accepts a bare code ("abcd efgh jk" -> "ABCDEFGHJK") or a deep link
 * ("fittrack://invite/<code>"). Returns null when it cannot be a valid code.
 */
export function parseInviteCode(input: string): string | null {
  let text = input.trim();
  const link = /^fittrack:\/\/invite\/([^/?#\s]+)/i.exec(text);
  if (link) text = link[1];
  const code = text.replace(/[\s-]/g, '').toUpperCase();
  return CODE_RE.test(code) ? code : null;
}

/** Text for the OS share sheet. */
export function inviteMessage(invite: Invite): string {
  return `Join me on FitTrack! Open ${invite.link} or enter code ${invite.code} on the Friends tab.`;
}

/** Plain-language labels for the summary screen; unknown keys fall back to a readable form. */
export function labelFor(key: string): string {
  const text = key.replace(/_/g, ' ').trim();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Primitive entries only (numbers, strings, booleans), skipping internal markers. */
export function summaryRows(summary: Record<string, unknown>): { label: string; value: string }[] {
  return Object.entries(summary)
    .filter(
      ([k, v]) =>
        k !== 'placeholder' &&
        k !== 'user_id' &&
        v !== null &&
        ['string', 'number', 'boolean'].includes(typeof v),
    )
    .map(([k, v]) => ({ label: labelFor(k), value: String(v) }));
}
