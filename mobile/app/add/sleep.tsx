import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Star } from 'lucide-react-native';
import { useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  defaultSleepWindow,
  durationMinutes,
  formatDuration,
  formatMoment,
  isValidWindow,
  listSleep,
  newSleepId,
  shiftMinutes,
} from '@/api/sleep';
import { saveSleep } from '@/sync/store';
import { goBack, refreshAfterSave } from '@/sync/hooks';
import { RecentEntries } from '@/sync/RecentEntries';
import { Body, Button, Caption, Screen, Title } from '@/components/ui';
import { Card } from '@/components/Card';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

const STEPS: { label: string; minutes: number }[] = [
  { label: '-1 h', minutes: -60 },
  { label: '-15', minutes: -15 },
  { label: '+15', minutes: 15 },
  { label: '+1 h', minutes: 60 },
];

function TimeStepper({
  title,
  value,
  now,
  onChange,
}: {
  title: string;
  value: Date;
  now: Date;
  onChange: (d: Date) => void;
}) {
  const t = useTheme();
  return (
    <View style={{ gap: spacing(1) }}>
      <View style={styles.rowBetween}>
        <Caption>{title}</Caption>
        <Text
          accessibilityLabel={`${title} time`}
          style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}
        >
          {formatMoment(value, now)}
        </Text>
      </View>
      <View style={styles.row}>
        {STEPS.map((s) => (
          <Pressable
            key={s.label}
            accessibilityRole="button"
            accessibilityLabel={`${title} ${s.label} minutes`}
            onPress={() => onChange(shiftMinutes(value, s.minutes))}
            style={[styles.step, { borderColor: t.border }]}
          >
            <Text style={{ fontSize: fontSizes.body, color: t.text }}>{s.label}</Text>
          </Pressable>
        ))}
      </View>
    </View>
  );
}

function Stars({ value, onChange }: { value: number | null; onChange: (v: number | null) => void }) {
  const t = useTheme();
  return (
    <View style={styles.row} accessibilityLabel="Sleep quality">
      {[1, 2, 3, 4, 5].map((n) => {
        const on = value !== null && n <= value;
        return (
          <Pressable
            key={n}
            accessibilityRole="button"
            accessibilityLabel={`${n} star${n === 1 ? '' : 's'}`}
            accessibilityState={{ selected: value === n }}
            // tapping the current rating again clears it (quality is optional)
            onPress={() => onChange(value === n ? null : n)}
            style={styles.star}
          >
            <Star size={32} color={on ? t.focus : t.muted} fill={on ? t.focus : 'none'} />
          </Pressable>
        );
      })}
    </View>
  );
}

export default function AddSleep() {
  const t = useTheme();
  const router = useRouter();
  const queryClient = useQueryClient();
  const [now] = useState(() => new Date());
  const [initial] = useState(() => defaultSleepWindow(now));
  const [bedtime, setBedtime] = useState(initial.bedtime);
  const [wake, setWake] = useState(initial.wake);
  const [quality, setQuality] = useState<number | null>(null);
  // One id per screen visit: a retried save updates the same row instead of duplicating it.
  const id = useRef(newSleepId());

  const recent = useQuery({ queryKey: ['sleep', 'recent'], queryFn: () => listSleep() });
  const minutes = durationMinutes(bedtime, wake);
  const valid = isValidWindow(bedtime, wake);

  const save = useMutation({
    mutationFn: () =>
      saveSleep({
        id: id.current,
        bedtime: bedtime.toISOString(),
        wake_time: wake.toISOString(),
        quality,
      }),
    onSuccess: () => {
      refreshAfterSave(queryClient, ['sleep']);
      goBack(router);
    },
  });

  const error = save.error
    ? save.error instanceof ApiError
      ? save.error.message
      : 'Could not save. Please try again.'
    : null;

  return (
    <Screen>
      <Title>Sleep</Title>
      <Card>
        <View style={{ gap: spacing(2) }}>
          <TimeStepper title="Bedtime" value={bedtime} now={now} onChange={setBedtime} />
          <TimeStepper title="Wake up" value={wake} now={now} onChange={setWake} />
        </View>
      </Card>
      <View style={{ alignItems: 'center', gap: spacing(0.5) }}>
        <Text
          accessibilityLabel="Duration"
          style={{ fontSize: fontSizes.title, fontWeight: '600', color: valid ? t.text : t.muted }}
        >
          {valid ? formatDuration(minutes) : '--'}
        </Text>
        {!valid ? <Caption warn>Wake up must be after bedtime, within 24 hours.</Caption> : null}
      </View>
      <View style={{ gap: spacing(1) }}>
        <Body muted>How did you sleep?</Body>
        <Stars value={quality} onChange={setQuality} />
      </View>
      {error ? <Caption warn>{error}</Caption> : null}
      <RecentEntries
        table="sleep"
        server={recent.data as unknown as { id: string; updated_at: string }[] | undefined}
        describe={(e) => {
          const mins = durationMinutes(new Date(String(e.data.bedtime)), new Date(String(e.data.wake_time)));
          return `${new Date(String(e.data.wake_time)).toLocaleDateString()} · ${formatDuration(mins)}`;
        }}
      />
      <Button
        title="Save"
        onPress={() => {
          if (valid) save.mutate();
        }}
        busy={save.isPending}
      />
    </Screen>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', gap: spacing(1) },
  rowBetween: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  step: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    minHeight: 44,
    borderWidth: 1,
    borderRadius: radius,
  },
  star: { flex: 1, alignItems: 'center', justifyContent: 'center', minHeight: 48 },
});
