import { StyleSheet, Text, View } from 'react-native';

import { Caption } from '@/components/ui';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

import { useRecentEntries } from './hooks';
import type { Entry } from './read';
import type { TableName } from './types';

/** Last few entries (server + this device), with a caption on ones still waiting to sync. */
export function RecentEntries({
  table,
  server,
  describe,
  title = 'Recent entries',
}: {
  table: TableName;
  server: ({ id: string; updated_at: string } & Record<string, unknown>)[] | undefined;
  describe: (e: Entry) => string;
  title?: string;
}) {
  const t = useTheme();
  const entries = useRecentEntries(table, server, 5);
  if (entries.length === 0) return null;
  return (
    <View style={{ gap: spacing(1) }}>
      <Caption>{title}</Caption>
      {entries.map((e) => (
        <View key={e.id} style={[styles.row, { borderColor: t.border, backgroundColor: t.card }]}>
          <Text numberOfLines={1} style={{ fontSize: fontSizes.body, color: t.text }}>
            {describe(e)}
          </Text>
          {e.pending ? <Caption>Waiting to sync</Caption> : null}
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { borderWidth: 1, borderRadius: radius, padding: spacing(1.5), gap: spacing(0.5) },
});
