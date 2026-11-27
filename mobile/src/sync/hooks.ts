import NetInfo from '@react-native-community/netinfo';
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import { onSynced } from './engine';
import { latestLocalWeight, pendingCount, pendingTotalsSince, recentEntries, type Entry, type LatestWeight, type PendingTotals } from './read';
import type { TableName } from './types';

/** Local-data queries live under this key so one invalidate refreshes them all. */
export const LOCAL_KEY = ['local'] as const;

function useRefreshOnSync() {
  const qc = useQueryClient();
  useEffect(() => onSynced(() => void qc.invalidateQueries({ queryKey: LOCAL_KEY })), [qc]);
}

/** False only when the device reports it is offline. */
export function useOnline(): boolean {
  const [online, setOnline] = useState(true);
  useEffect(() => {
    const unsub = NetInfo.addEventListener((s) => setOnline(s.isConnected !== false));
    return () => unsub();
  }, []);
  return online;
}

export function usePendingCount(): number {
  useRefreshOnSync();
  const q = useQuery({ queryKey: [...LOCAL_KEY, 'pending'], queryFn: pendingCount });
  return q.data ?? 0;
}

export function usePendingTotals(since: Date): PendingTotals {
  useRefreshOnSync();
  const day = since.toISOString();
  const q = useQuery({
    queryKey: [...LOCAL_KEY, 'totals', day],
    queryFn: () => pendingTotalsSince(new Date(day)),
  });
  return q.data ?? { kcal: 0, minutes: 0 };
}

export function useRecentEntries(
  table: TableName,
  server: ({ id: string; updated_at: string } & Record<string, unknown>)[] | undefined,
  limit = 5,
): Entry[] {
  useRefreshOnSync();
  const q = useQuery({
    queryKey: [...LOCAL_KEY, 'recent', table, server?.map((s) => `${s.id}@${s.updated_at}`).join(',')],
    queryFn: () => recentEntries(table, server ?? [], limit),
  });
  return q.data ?? [];
}

export function useLatestLocalWeight(): LatestWeight | null {
  useRefreshOnSync();
  const q = useQuery({ queryKey: [...LOCAL_KEY, 'weight-latest'], queryFn: latestLocalWeight });
  return q.data ?? null;
}

/**
 * After a local save: refresh every local-data query, the dashboard and the screen's own
 * list right away (not awaited, so navigation is instant), including inactive ones such
 * as the Home tab sitting underneath the pushed screen.
 */
export function refreshAfterSave(qc: QueryClient, domainKey: readonly unknown[]): void {
  for (const queryKey of [LOCAL_KEY, ['dashboard'], domainKey]) {
    void qc.invalidateQueries({ queryKey: [...queryKey], refetchType: 'all' });
  }
}

/** Back to the previous screen, or Home when there is none (deep link). */
export function goBack(router: {
  canGoBack: () => boolean;
  back: () => void;
  replace: (p: '/') => void;
}): void {
  if (router.canGoBack()) router.back();
  else router.replace('/');
}
