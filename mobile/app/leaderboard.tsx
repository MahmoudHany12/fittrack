import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { ScrollView, StyleSheet, Switch, Text, View } from 'react-native';

import {
  barFraction,
  getLeaderboard,
  getOptIn,
  leaderboardKeys,
  setOptIn,
  totalLabel,
  type LeaderboardEntry,
  type StatValue,
} from '@/api/leaderboard';
import { Card } from '@/components/Card';
import { Body, Caption, Title } from '@/components/ui';
import { fontSizes, spacing, useTheme } from '@/theme';

function StatBar({ label, stat, color }: { label: string; stat: StatValue | null; color: string }) {
  const t = useTheme();
  return (
    <View style={styles.barRow} accessibilityLabel={`${label} level ${stat?.level ?? 0}`}>
      <Text style={[styles.barLabel, { color: t.muted }]}>{label}</Text>
      <View style={[styles.track, { backgroundColor: t.border }]}>
        <View style={{ width: `${barFraction(stat) * 100}%`, height: 6, backgroundColor: color }} />
      </View>
      <Text style={[styles.barLevel, { color: t.muted }]}>{stat ? `Lv ${stat.level}` : '-'}</Text>
    </View>
  );
}

function Row({ entry }: { entry: LeaderboardEntry }) {
  const t = useTheme();
  return (
    <Card>
      <View style={styles.head}>
        <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
          {entry.rank}. {entry.display_name}
          {entry.is_me ? ' (you)' : ''}
        </Text>
        <Caption>{totalLabel(entry)}</Caption>
      </View>
      <StatBar label="Strength" stat={entry.strength} color={t.strength} />
      <StatBar label="Cardio" stat={entry.cardio} color={t.cardio} />
      <StatBar label="Focus" stat={entry.focus} color={t.focus} />
    </Card>
  );
}

export default function Leaderboard() {
  const t = useTheme();
  const qc = useQueryClient();
  const optin = useQuery({ queryKey: leaderboardKeys.optin, queryFn: getOptIn });
  const joined = optin.data?.opted_in === true;
  const board = useQuery({
    queryKey: leaderboardKeys.board,
    queryFn: getLeaderboard,
    enabled: joined,
  });
  const toggle = useMutation({
    mutationFn: setOptIn,
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: leaderboardKeys.board });
    },
  });

  return (
    <ScrollView style={{ backgroundColor: t.bg }} contentContainerStyle={styles.content}>
      <Title>Leaderboard</Title>
      <Card>
        <View style={styles.head}>
          <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
            Join the leaderboard
          </Text>
          <Switch
            accessibilityLabel="Join the leaderboard"
            value={joined}
            disabled={optin.isPending || toggle.isPending}
            onValueChange={(v) => toggle.mutate(v)}
          />
        </View>
        <Body muted>
          Only friends who also join can see you. You only see friends who joined too. Turn it off any
          time.
        </Body>
        {toggle.isError ? <Caption warn>Could not save your choice. Try again.</Caption> : null}
      </Card>

      {optin.isError ? <Caption warn>Could not load your setting.</Caption> : null}
      {joined && board.isPending ? <Caption>Loading...</Caption> : null}
      {joined && board.isError ? <Caption warn>Could not load the leaderboard.</Caption> : null}
      {joined && board.data?.length === 1 ? (
        <Caption>No friends have joined yet. Invite some from the Friends tab.</Caption>
      ) : null}
      {joined && board.data ? board.data.map((e) => <Row key={e.user_id} entry={e} />) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), gap: spacing(2) },
  head: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    gap: spacing(1),
  },
  barRow: { flexDirection: 'row', alignItems: 'center', gap: spacing(1), marginTop: spacing(1) },
  barLabel: { width: 64, fontSize: fontSizes.caption },
  track: { flex: 1, height: 6, borderRadius: 3, overflow: 'hidden' },
  barLevel: { width: 40, textAlign: 'right', fontSize: fontSizes.caption },
});
