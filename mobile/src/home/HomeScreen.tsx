import { useQuery } from '@tanstack/react-query';
import { Activity, Brain, Dumbbell } from 'lucide-react-native';
import { useRouter } from 'expo-router';
import { useState, type ReactNode } from 'react';
import { Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';

import { getDashboard, type Dashboard, type StatEntry } from '@/api/dashboard';
import { Ring } from '@/components/Ring';
import { Body, Button, Caption, Title } from '@/components/ui';
import { useOnline, usePendingCount, usePendingTotals } from '@/sync/hooks';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

import {
  dietColor,
  formatHours,
  formatKg,
  formatNumber,
  kcalFraction,
  statView,
  statusColor,
  trendArrow,
  trendWord,
} from './helpers';

type Route = '/add/workout' | '/add/food' | '/add/sleep' | '/add/weight';

function SectionHeading({ children }: { children: string }) {
  const t = useTheme();
  return (
    <Text
      accessibilityRole="header"
      style={{ fontSize: fontSizes.body, fontWeight: '600', color: t.muted, marginTop: spacing(1) }}
    >
      {children}
    </Text>
  );
}

/** Flat tappable surface with an accessibility label (one tap to a quick-log screen). */
function Tap({
  label,
  onPress,
  children,
  card,
}: {
  label: string;
  onPress: () => void;
  children: ReactNode;
  card?: boolean;
}) {
  const t = useTheme();
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={onPress}
      style={card ? [styles.card, { backgroundColor: t.card, borderColor: t.border }] : styles.rowTap}
    >
      {children}
    </Pressable>
  );
}

function Bar({ fraction, color }: { fraction: number; color: string }) {
  const t = useTheme();
  return (
    <View style={[styles.barTrack, { backgroundColor: t.border }]}>
      <View
        style={{ width: `${Math.round(fraction * 100)}%`, height: '100%', backgroundColor: color }}
      />
    </View>
  );
}

function StatCard({
  title,
  icon,
  color,
  stat,
  onPress,
}: {
  title: string;
  icon: typeof Dumbbell;
  color: string;
  stat: StatEntry;
  onPress: () => void;
}) {
  const t = useTheme();
  const { level, progress } = statView(stat);
  return (
    <Tap card label={`${title}, level ${level} of 4. Tap to log.`} onPress={onPress}>
      <View style={styles.row}>
        <View>
          <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
            {title}
          </Text>
          <Caption>{`Level ${level}/4`}</Caption>
        </View>
        <Ring progress={progress} icon={icon} color={color} />
      </View>
    </Tap>
  );
}

function ActivityCard({ data, onPress }: { data: Dashboard; onPress: () => void }) {
  const t = useTheme();
  const ex = data.exercise;
  const steps = ex?.steps;
  const minutes = ex?.workout_minutes_today;
  const empty = steps == null && minutes == null;
  const summary = empty
    ? 'No activity yet — tap to add'
    : `${formatNumber(steps)} steps · ${minutes ?? 0} min workout today`;
  return (
    <Tap card label={`Activity. ${summary}`} onPress={onPress}>
      <View style={styles.row}>
        <View style={{ flex: 1, paddingRight: spacing(1) }}>
          <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
            Activity
          </Text>
          <Caption>{summary}</Caption>
        </View>
        <Ring
          progress={minutes ? Math.min(1, minutes / 30) : 0}
          icon={Activity}
          color={t.cardio}
        />
      </View>
    </Tap>
  );
}

function BodyRows({ data, go }: { data: Dashboard; go: (r: Route) => void }) {
  const t = useTheme();
  const sleep = data.sleep;
  const diet = data.diet;
  const weight = data.weight;
  const hasSleep = sleep?.minutes != null;
  const hasDiet = diet?.kcal_eaten != null && diet?.kcal_target != null;
  const hasWeight = weight?.kg != null;

  const sleepText = hasSleep ? formatHours(sleep?.minutes) : 'No sleep logged yet — tap to add';
  const dietText = hasDiet
    ? `${formatNumber(diet?.kcal_eaten)} / ${formatNumber(diet?.kcal_target)} kcal`
    : 'No meals logged yet — tap to add';
  const weightText = hasWeight ? formatKg(weight?.kg as number) : 'No weight logged yet — tap to add';
  const arrow = trendArrow(weight?.trend);

  return (
    <View style={[styles.card, { backgroundColor: t.card, borderColor: t.border }]}>
      <Tap
        label={hasSleep ? `Sleep, ${sleepText}${sleep?.label ? `, ${sleep.label}` : ''}. Tap to log.` : `Sleep. ${sleepText}`}
        onPress={() => go('/add/sleep')}
      >
        <View style={styles.row}>
          <Text style={[styles.rowName, { color: t.text }]}>Sleep</Text>
          <View style={styles.rowValue}>
            <Body muted={!hasSleep}>{sleepText}</Body>
            {hasSleep && sleep?.label ? (
              <Text style={{ fontSize: fontSizes.caption, color: statusColor(data.status.key, t) }}>
                {sleep.label}
              </Text>
            ) : null}
          </View>
        </View>
      </Tap>
      <View style={[styles.divider, { backgroundColor: t.border }]} />
      <Tap
        label={hasDiet ? `Diet, ${dietText}${diet?.label ? `, ${diet.label}` : ''}. Tap to log.` : `Diet. ${dietText}`}
        onPress={() => go('/add/food')}
      >
        <View style={styles.row}>
          <Text style={[styles.rowName, { color: t.text }]}>Diet</Text>
          <View style={styles.rowValue}>
            <Body muted={!hasDiet}>{dietText}</Body>
            {hasDiet ? (
              <>
                <Bar
                  fraction={kcalFraction(diet?.kcal_eaten, diet?.kcal_target)}
                  color={dietColor(diet?.label, t)}
                />
                {diet?.label ? (
                  <Text style={{ fontSize: fontSizes.caption, color: dietColor(diet.label, t) }}>
                    {diet.label}
                  </Text>
                ) : null}
              </>
            ) : null}
          </View>
        </View>
      </Tap>
      <View style={[styles.divider, { backgroundColor: t.border }]} />
      <Tap
        label={
          hasWeight
            ? `Weight, ${weightText}${weight?.trend ? `, ${trendWord(weight.trend)}` : ''}. Tap to log.`
            : `Weight. ${weightText}`
        }
        onPress={() => go('/add/weight')}
      >
        <View style={styles.row}>
          <Text style={[styles.rowName, { color: t.text }]}>Weight</Text>
          <View style={[styles.rowValue, styles.weightValue]}>
            <Body muted={!hasWeight}>{weightText}</Body>
            {hasWeight && arrow ? (
              <Text style={{ fontSize: fontSizes.cardTitle, color: t.text }}>{arrow}</Text>
            ) : null}
          </View>
        </View>
      </Tap>
    </View>
  );
}

/** Start of today in UTC (matches how the dashboard counts "today"). */
function startOfTodayUtc(): Date {
  const d = new Date();
  d.setUTCHours(0, 0, 0, 0);
  return d;
}

/** Add today's unsynced local food kcal / workout minutes on top of the server numbers. */
export function withPendingTotals(data: Dashboard, extra: { kcal: number; minutes: number }): Dashboard {
  if (!extra.kcal && !extra.minutes) return data;
  const diet = data.diet;
  const ex = data.exercise;
  return {
    ...data,
    diet:
      extra.kcal && diet
        ? { ...diet, kcal_eaten: (diet.kcal_eaten ?? 0) + extra.kcal }
        : diet,
    exercise:
      extra.minutes && ex
        ? { ...ex, workout_minutes_today: (ex.workout_minutes_today ?? 0) + extra.minutes }
        : ex,
  };
}

export function HomeScreen() {
  const t = useTheme();
  const online = useOnline();
  const pending = usePendingCount();
  const [today] = useState(startOfTodayUtc);
  const extra = usePendingTotals(today);
  const router = useRouter();
  const { data, error, isPending, isError, isRefetching, refetch } = useQuery({
    queryKey: ['dashboard'],
    queryFn: getDashboard,
  });
  const go = (r: Route) => router.push(r);

  const refresh = (
    <RefreshControl refreshing={isRefetching} onRefresh={() => void refetch()} tintColor={t.muted} />
  );

  if (!data) {
    return (
      <ScrollView
        style={{ backgroundColor: t.bg }}
        contentContainerStyle={styles.content}
        refreshControl={refresh}
      >
        <Title>Home</Title>
        {isPending ? <Caption>Loading…</Caption> : null}
        {isError ? (
          <View style={{ gap: spacing(1.5) }}>
            <Body muted>
              {(error as { code?: string } | null)?.code === 'network'
                ? "We can't reach the server right now. Check your connection and try again."
                : "We couldn't load your day. Please try again."}
            </Body>
            <Button title="Retry" onPress={() => void refetch()} variant="plain" />
          </View>
        ) : null}
      </ScrollView>
    );
  }

  const shown = withPendingTotals(data, extra);
  const stats = data.stats;
  return (
    <ScrollView
      style={{ backgroundColor: t.bg }}
      contentContainerStyle={styles.content}
      refreshControl={refresh}
    >
      <View>
        <Title>{data.name || 'Home'}</Title>
        <Text
          accessibilityLabel={`Status: ${data.status.label}`}
          style={{ fontSize: fontSizes.cardTitle, color: statusColor(data.status.key, t) }}
        >
          {data.status.label}
        </Text>
        {isError || !online ? <Caption>Saved on this device</Caption> : null}
        {pending > 0 ? (
          <Caption>{`${pending} ${pending === 1 ? 'entry' : 'entries'} waiting to sync`}</Caption>
        ) : null}
      </View>

      <SectionHeading>Fitness</SectionHeading>
      {stats ? (
        <>
          <StatCard
            title="Strength"
            icon={Dumbbell}
            color={t.strength}
            stat={stats.strength}
            onPress={() => go('/add/workout')}
          />
          <StatCard
            title="Cardio"
            icon={Activity}
            color={t.cardio}
            stat={stats.cardio}
            onPress={() => go('/add/workout')}
          />
          <StatCard
            title="Focus"
            icon={Brain}
            color={t.focus}
            stat={stats.focus}
            onPress={() => go('/add/sleep')}
          />
        </>
      ) : (
        <ActivityCard data={shown} onPress={() => go('/add/workout')} />
      )}

      <SectionHeading>Body & Nutrition</SectionHeading>
      <BodyRows data={shown} go={go} />
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), paddingTop: spacing(7), gap: spacing(1.5) },
  card: { borderWidth: 1, borderRadius: radius, padding: spacing(2) },
  row: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  rowTap: { paddingVertical: spacing(1), minHeight: 48, justifyContent: 'center' },
  rowName: { fontSize: fontSizes.body, fontWeight: '600', width: 64 },
  rowValue: { flex: 1, alignItems: 'flex-end', gap: spacing(0.5) },
  weightValue: { flexDirection: 'row', justifyContent: 'flex-end', alignItems: 'center', gap: spacing(1) },
  divider: { height: 1 },
  barTrack: { height: 6, width: '100%', borderRadius: 3, overflow: 'hidden' },
});
