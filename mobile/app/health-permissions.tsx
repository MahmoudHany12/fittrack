import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { ScrollView, StyleSheet, Switch, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  HEALTH_CATEGORIES,
  getConsents,
  putConsents,
  type HealthCategory,
  type HealthConsents,
} from '@/api/health';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Screen, Title } from '@/components/ui';
import { getHealthProvider } from '@/health/provider';
import { loadLastSync, runSync, saveLastSync, type SyncSummary } from '@/health/sync';
import { fontSizes, spacing, useTheme } from '@/theme';

const COPY: Record<HealthCategory, { title: string; text: string }> = {
  steps: {
    title: 'Steps',
    text: 'Your daily step count. Imported once per day, so we never store minute-by-minute data.',
  },
  workouts: {
    title: 'Workouts',
    text: 'The start time, length and type (for example running) of each workout. No routes or heart rate.',
  },
  sleep: {
    title: 'Sleep',
    text: 'When you went to sleep and woke up. No sleep stages.',
  },
};

const OFF: HealthConsents = { steps: false, workouts: false, sleep: false };

export function describeSummary(s: SyncSummary): string {
  const t = s.totals;
  const imported = t.steps.imported + t.workouts.imported + t.sleep.imported;
  if (imported === 0) return 'Everything is already up to date.';
  const parts = [
    t.steps.imported ? `${t.steps.imported} step day${t.steps.imported === 1 ? '' : 's'}` : null,
    t.workouts.imported ? `${t.workouts.imported} workout${t.workouts.imported === 1 ? '' : 's'}` : null,
    t.sleep.imported ? `${t.sleep.imported} sleep${t.sleep.imported === 1 ? '' : 's'}` : null,
  ].filter(Boolean);
  return `Imported ${parts.join(', ')}.`;
}

export default function HealthPermissions() {
  const t = useTheme();
  const queryClient = useQueryClient();
  const provider = getHealthProvider();
  const [message, setMessage] = useState<string | null>(null);

  const consents = useQuery({ queryKey: ['health', 'consents'], queryFn: getConsents });
  const lastSync = useQuery({ queryKey: ['health', 'lastSync'], queryFn: loadLastSync });
  const current = consents.data ?? OFF;

  const toggle = useMutation({
    mutationFn: async ({ category, on }: { category: HealthCategory; on: boolean }) => {
      if (on) {
        const ok = await provider.requestPermissions([category]);
        if (!ok) {
          throw new Error(
            provider.available
              ? 'Permission was not given in the system dialog. You can change it in your phone settings.'
              : 'Health data needs the FitTrack app build for your phone (not Expo Go or the web).',
          );
        }
      }
      return putConsents({ ...current, [category]: on });
    },
    onSuccess: (data) => {
      setMessage(null);
      queryClient.setQueryData(['health', 'consents'], data);
    },
    onError: (e) => setMessage(e instanceof Error ? e.message : 'Could not update. Try again.'),
  });

  const sync = useMutation({
    mutationFn: async () => {
      const now = new Date();
      const summary = await runSync({
        provider,
        consents: current,
        lastSync: await loadLastSync(),
        now,
      });
      await saveLastSync(now);
      return summary;
    },
    onSuccess: async (summary) => {
      setMessage(describeSummary(summary));
      await queryClient.invalidateQueries({ queryKey: ['health', 'lastSync'] });
      for (const key of ['dashboard', 'sleep', 'workouts']) {
        await queryClient.invalidateQueries({ queryKey: [key] });
      }
    },
    onError: (e) =>
      setMessage(e instanceof ApiError ? e.message : 'Could not sync. Please try again.'),
  });

  const anyOn = HEALTH_CATEGORIES.some((c) => current[c]);
  const busy = toggle.isPending;

  return (
    <Screen>
      <ScrollView contentContainerStyle={{ gap: spacing(2) }}>
        <Title>Health data</Title>
        <Body muted>
          Choose what FitTrack may read from {provider.available ? 'your phone' : 'the health app'}.
          Each type is separate and off until you turn it on. You can turn one off at any time;
          that stops new imports and keeps what was already imported.
        </Body>
        {!provider.available ? (
          <Caption warn>
            Automatic import from Apple Health or Health Connect needs the installed FitTrack app
            (TestFlight or an APK). It can&apos;t run inside Expo Go. You can still log everything
            by hand.
          </Caption>
        ) : null}
        {HEALTH_CATEGORIES.map((c) => (
          <Card key={c}>
            <View style={styles.row}>
              <View style={{ flex: 1, gap: spacing(0.5) }}>
                <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
                  {COPY[c].title}
                </Text>
                <Caption>{COPY[c].text}</Caption>
              </View>
              <Switch
                accessibilityLabel={`Import ${COPY[c].title.toLowerCase()}`}
                value={current[c]}
                disabled={busy || consents.isLoading || consents.isError}
                onValueChange={(on) => toggle.mutate({ category: c, on })}
                trackColor={{ true: t.accent, false: t.border }}
              />
            </View>
          </Card>
        ))}
        {consents.isError ? <Caption warn>Could not load your settings.</Caption> : null}
        {message ? <Caption warn={sync.isError || toggle.isError}>{message}</Caption> : null}
        <Button
          title="Sync now"
          onPress={() => {
            if (anyOn) sync.mutate();
            else setMessage('Turn on at least one type first.');
          }}
          busy={sync.isPending}
        />
        <Caption>
          {lastSync.data
            ? `Last synced ${lastSync.data.toLocaleString()}`
            : 'Not synced yet. The first sync reads the last 30 days.'}
        </Caption>
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: spacing(2) },
});
