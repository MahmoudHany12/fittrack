import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { shareExport, type ExportRequest } from '@/api/export';
import { getProfile, profileKeys } from '@/api/profile';
import {
  dietSentence,
  formatRange,
  getWeeklySummary,
  isEmptyWeek,
  sleepSentence,
  summaryKeys,
  trainingSentence,
  weightSentence,
} from '@/api/summary';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Title } from '@/components/ui';
import { fontSizes, spacing, useTheme } from '@/theme';

function Section({ title, color, children }: { title: string; color: string; children: string }) {
  const t = useTheme();
  return (
    <Card>
      <View style={styles.cardHead}>
        <View style={[styles.dot, { backgroundColor: color }]} />
        <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>{title}</Text>
      </View>
      <Body>{children}</Body>
    </Card>
  );
}

export default function Summary() {
  const t = useTheme();
  const summary = useQuery({ queryKey: summaryKeys.weekly(), queryFn: () => getWeeklySummary() });
  const profile = useQuery({ queryKey: profileKeys.all, queryFn: getProfile });
  const [busy, setBusy] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  const doExport = async (key: string, req: ExportRequest) => {
    setBusy(key);
    setExportError(null);
    try {
      await shareExport(req);
    } catch (e) {
      setExportError(e instanceof Error ? e.message : 'Could not export your data.');
    } finally {
      setBusy(null);
    }
  };

  const s = summary.data;
  return (
    <ScrollView
      style={{ backgroundColor: t.bg }}
      contentContainerStyle={styles.content}
      accessibilityLabel="Weekly summary"
    >
      <Title>Your week</Title>
      {s ? <Caption>{formatRange(s.from, s.to)}</Caption> : null}

      {summary.isPending ? <Caption>Loading...</Caption> : null}
      {summary.isError ? <Caption warn>Could not load your summary. Pull back and try again.</Caption> : null}

      {s && isEmptyWeek(s) ? (
        <Card>
          <Body>Nothing logged this week yet. Log a workout, a meal, your sleep or your weight and it will show up here.</Body>
        </Card>
      ) : null}
      {s && !isEmptyWeek(s) ? (
        <>
          <Section title="Training" color={t.strength}>
            {trainingSentence(s.training)}
          </Section>
          <Section title="Sleep" color={t.focus}>
            {sleepSentence(s.sleep)}
          </Section>
          <Section title="Weight" color={t.cardio}>
            {weightSentence(s.weight, profile.data?.units ?? 'metric')}
          </Section>
          <Section title="Food" color={t.warn}>
            {dietSentence(s.diet)}
          </Section>
        </>
      ) : null}

      <View style={styles.exportBlock}>
        <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>Export your data</Text>
        <Caption>Your own data only. Open it in another app or keep it as a backup.</Caption>
        <Button
          title="Share everything (JSON)"
          variant="plain"
          busy={busy === 'json'}
          onPress={() => void doExport('json', { format: 'json' })}
        />
        <Button
          title="Share workouts (CSV)"
          variant="plain"
          busy={busy === 'workouts'}
          onPress={() => void doExport('workouts', { format: 'csv', table: 'workouts' })}
        />
        <Button
          title="Share food (CSV)"
          variant="plain"
          busy={busy === 'food'}
          onPress={() => void doExport('food', { format: 'csv', table: 'food' })}
        />
        <Button
          title="Share sleep (CSV)"
          variant="plain"
          busy={busy === 'sleep'}
          onPress={() => void doExport('sleep', { format: 'csv', table: 'sleep' })}
        />
        <Button
          title="Share weight (CSV)"
          variant="plain"
          busy={busy === 'weight'}
          onPress={() => void doExport('weight', { format: 'csv', table: 'weight' })}
        />
        {exportError ? <Caption warn>{exportError}</Caption> : null}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), gap: spacing(2) },
  cardHead: { flexDirection: 'row', alignItems: 'center', gap: spacing(1), marginBottom: spacing(1) },
  dot: { width: 10, height: 10, borderRadius: 5 },
  exportBlock: { gap: spacing(1), marginTop: spacing(2) },
});
