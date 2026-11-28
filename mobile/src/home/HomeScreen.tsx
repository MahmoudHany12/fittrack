import { useQuery } from '@tanstack/react-query';
import { Award, Eye, Heart, Hourglass, Zap, type LucideIcon } from 'lucide-react-native';
import { useRouter } from 'expo-router';
import { useState, type ReactNode } from 'react';
import {
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  Text,
  View,
  type StyleProp,
  type ViewStyle,
} from 'react-native';

import { getDashboard, type Dashboard, type StatEntry } from '@/api/dashboard';
import type { Progression, WindowTrack } from '@/api/progression';
import { Ring } from '@/components/Ring';
import { Body, Button, Caption, Title } from '@/components/ui';
import { useLatestLocalWeight, useOnline, usePendingCount, usePendingTotals } from '@/sync/hooks';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

import {
  dietColor,
  formatHours,
  formatKg,
  formatNumber,
  initials,
  kcalFraction,
  pickWeight,
  statView,
  statusColor,
  statusWord,
  statusWordColor,
  trendArrow,
  trendWord,
} from './helpers';
import { FOCUS_TEXT, pickHints, ringText, trackView, windowView } from './progression';

type Route =
  | '/add/workout'
  | '/add/food'
  | '/add/sleep'
  | '/add/weight'
  | '/records'
  | '/program';

function SectionHeading({ children }: { children: string }) {
  const t = useTheme();
  return (
    <Text
      accessibilityRole="header"
      style={{ fontSize: fontSizes.caption, fontWeight: '600', color: t.muted, marginTop: spacing(1) }}
    >
      {children}
    </Text>
  );
}

/** Flat tappable surface with an accessibility label (one tap to a quick-log screen). */
function Tap({
  label,
  onPress,
  onLongPress,
  children,
  style,
}: {
  label: string;
  onPress: () => void;
  onLongPress?: () => void;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
}) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={onPress}
      onLongPress={onLongPress}
      style={style}
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
  onLongPress,
  outlined,
  track,
  children,
}: {
  title: string;
  icon: LucideIcon;
  color: string;
  stat: StatEntry | null | undefined;
  onPress: () => void;
  onLongPress?: () => void;
  outlined?: boolean;
  /** Level from the progression model; wins over `stat` when present. */
  track?: { level: number; progress: number } | null;
  children?: ReactNode;
}) {
  const t = useTheme();
  const { level, progress } = track ?? statView(stat);
  return (
    <View
      style={[styles.statCard, { backgroundColor: t.card, borderColor: outlined ? t.muted : t.card }]}
    >
      <Tap
        label={`${title}, level ${level} of 4. Tap to log.`}
        onPress={onPress}
        onLongPress={onLongPress}
        style={styles.statTap}
      >
        <View>
          <Text style={{ fontSize: fontSizes.cardTitle + 2, fontWeight: '700', color }}>{title}</Text>
          <Caption>{`Level ${level}/4`}</Caption>
        </View>
        <Ring progress={progress} icon={icon} color={color} size={76} strokeWidth={7} />
      </Tap>
      {children}
    </View>
  );
}

function WorkoutWindow({
  fraction,
  minutes,
  goal,
  onPress,
}: {
  fraction: number;
  minutes: number;
  goal: number;
  onPress: () => void;
}) {
  const t = useTheme();
  return (
    <Tap
      label={`Workout window, ${minutes} of ${goal} minutes. Tap to log.`}
      onPress={onPress}
      style={[styles.inset, { backgroundColor: t.inset }]}
    >
      <View style={styles.windowHead}>
        <Hourglass size={18} color={t.cardio} />
        <Text style={{ fontSize: fontSizes.body, fontWeight: '600', color: t.text }}>
          Workout window
        </Text>
        <Caption>{`${minutes} / ${goal} min`}</Caption>
      </View>
      <Bar fraction={fraction} color={t.cardio} />
    </Tap>
  );
}

/** Window state for a strength/cardio track: draining or filling bar plus one plain line. */
function ProgressionWindow({
  track,
  color,
  onPress,
}: {
  track: WindowTrack;
  color: string;
  onPress: () => void;
}) {
  const t = useTheme();
  const v = windowView(track);
  return (
    <Tap
      label={`Workout window. ${v.text}. Tap to log.`}
      onPress={onPress}
      style={[styles.inset, { backgroundColor: t.inset }]}
    >
      <View style={styles.windowHead}>
        <Hourglass size={18} color={color} />
        <Text style={{ fontSize: fontSizes.body, fontWeight: '600', color: t.text }}>
          Workout window
        </Text>
        {v.proteinBonus ? (
          <View style={[styles.chip, { borderColor: t.good }]}>
            <Text style={{ fontSize: fontSizes.caption, color: t.good }}>Protein bonus</Text>
          </View>
        ) : null}
      </View>
      <Bar fraction={v.fraction} color={v.warn ? t.warn : color} />
      {v.text ? <Caption warn={v.warn}>{v.text}</Caption> : null}
    </Tap>
  );
}

function DailyRing({ ring, color }: { ring: Progression['ring']; color: string }) {
  const r = ringText(ring);
  return (
    <View style={styles.dailyRing} accessibilityLabel={`Daily ring: ${r.label}`}>
      <Ring progress={r.fraction} icon={Zap} color={color} size={36} strokeWidth={4} />
      <Caption>{r.label}</Caption>
    </View>
  );
}

function BodyRow({
  name,
  label,
  onPress,
  children,
}: {
  name: string;
  label: string;
  onPress: () => void;
  children: ReactNode;
}) {
  const t = useTheme();
  return (
    <Tap label={label} onPress={onPress} style={[styles.bodyRow, { backgroundColor: t.card }]}>
      <Text style={[styles.rowName, { color: t.text }]}>{name}</Text>
      <View style={styles.rowValue}>{children}</View>
    </Tap>
  );
}

function BodyRows({
  data,
  go,
  weightKg,
}: {
  data: Dashboard;
  go: (r: Route) => void;
  weightKg: number | null;
}) {
  const t = useTheme();
  const sleep = data.sleep;
  const diet = data.diet;
  const trend = data.weight?.trend ?? data.body?.trend;
  const hasSleep = sleep?.minutes != null;
  const hasDiet = diet?.kcal_eaten != null && diet?.kcal_target != null;
  const hasWeight = weightKg != null;

  const sleepText = hasSleep ? formatHours(sleep?.minutes) : 'No sleep logged yet — tap to add';
  const dietText = hasDiet
    ? `${formatNumber(diet?.kcal_eaten)} / ${formatNumber(diet?.kcal_target)} kcal`
    : 'No meals logged yet — tap to add';
  const weightText = hasWeight ? formatKg(weightKg) : 'No weight logged yet — tap to add';
  const arrow = trendArrow(trend);
  const fat = data.body?.body_fat_pct;
  const muscle = data.body?.skeletal_muscle_kg;
  const composition = [
    fat != null ? `Body fat ${Math.round(fat * 10) / 10}%` : null,
    muscle != null ? `Muscle ${Math.round(muscle * 10) / 10} kg` : null,
  ]
    .filter(Boolean)
    .join(' · ');
  const proteinEaten = diet?.protein_g_eaten;
  const proteinTarget = diet?.protein_target_g;
  const protein =
    proteinEaten != null && proteinTarget != null
      ? `Protein ${formatNumber(proteinEaten)} / ${formatNumber(proteinTarget)} g`
      : null;

  return (
    <View style={{ gap: spacing(1) }}>
      <BodyRow
        name="Weight"
        label={
          hasWeight
            ? `Weight, ${weightText}${trend ? `, ${trendWord(trend)}` : ''}. Tap to log.`
            : `Weight. ${weightText}`
        }
        onPress={() => go('/add/weight')}
      >
        <View style={styles.weightValue}>
          <Body muted={!hasWeight}>{weightText}</Body>
          {hasWeight && arrow ? (
            <Text style={{ fontSize: fontSizes.cardTitle, color: t.text }}>{arrow}</Text>
          ) : null}
        </View>
        {composition ? <Caption>{composition}</Caption> : null}
      </BodyRow>
      <BodyRow
        name="Diet"
        label={
          hasDiet
            ? `Diet, ${dietText}${diet?.label ? `, ${diet.label}` : ''}. Tap to log.`
            : `Diet. ${dietText}`
        }
        onPress={() => go('/add/food')}
      >
        <Body muted={!hasDiet}>{dietText}</Body>
        {hasDiet ? (
          <Bar
            fraction={kcalFraction(diet?.kcal_eaten, diet?.kcal_target)}
            color={dietColor(diet?.label, t)}
          />
        ) : null}
        {protein ? <Caption>{protein}</Caption> : null}
      </BodyRow>
      <BodyRow
        name="Sleep"
        label={
          hasSleep
            ? `Sleep, ${sleepText}${sleep?.label ? `, ${sleep.label}` : ''}. Tap to log.`
            : `Sleep. ${sleepText}`
        }
        onPress={() => go('/add/sleep')}
      >
        <Body muted={!hasSleep}>{sleepText}</Body>
        {hasSleep && sleep?.label ? (
          <Text style={{ fontSize: fontSizes.caption, color: statusColor(data.status.key, t) }}>
            {sleep.label}
          </Text>
        ) : null}
      </BodyRow>
    </View>
  );
}

/** Start of today in UTC (matches how the dashboard counts "today"). */
function startOfTodayUtc(): Date {
  const d = new Date();
  d.setUTCHours(0, 0, 0, 0);
  return d;
}

function windowWith(minutes: number, goal: number) {
  const g = goal > 0 ? goal : 30;
  return { minutes_today: minutes, goal_minutes: g, fraction: Math.min(1, Math.max(0, minutes / g)) };
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
    workout_window:
      extra.minutes && data.workout_window
        ? windowWith(data.workout_window.minutes_today + extra.minutes, data.workout_window.goal_minutes)
        : data.workout_window,
  };
}

export function HomeScreen() {
  const t = useTheme();
  const online = useOnline();
  const pending = usePendingCount();
  const [today] = useState(startOfTodayUtc);
  const extra = usePendingTotals(today);
  const localWeight = useLatestLocalWeight();
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
  const word = statusWord(data.status.key, data.readiness?.score);
  const win = shown.workout_window ?? windowWith(shown.exercise?.workout_minutes_today ?? 0, 30);
  const prog = data.progression ?? null;
  const hints = pickHints(prog?.hints);
  const newBests = prog?.new_personal_bests ?? [];
  const weightKg = pickWeight(data.body?.weight_kg ?? data.weight?.kg, localWeight);
  return (
    <ScrollView
      style={{ backgroundColor: t.bg }}
      contentContainerStyle={styles.content}
      refreshControl={refresh}
    >
      <View style={styles.header}>
        <View style={{ flex: 1 }}>
          <Text style={{ fontSize: fontSizes.title + 4, fontWeight: '700', color: t.text }}>
            {data.name || 'Home'}
          </Text>
          <Text
            accessibilityLabel={`Status: ${word}`}
            style={{ fontSize: fontSizes.cardTitle, color: statusWordColor(word, t) }}
          >
            {word}
          </Text>
          {isError || !online ? <Caption>Saved on this device</Caption> : null}
          {pending > 0 ? (
            <Caption>{`${pending} ${pending === 1 ? 'entry' : 'entries'} waiting to sync`}</Caption>
          ) : null}
        </View>
        {prog?.ring ? <DailyRing ring={prog.ring} color={t.cardio} /> : null}
        <View style={[styles.avatar, { backgroundColor: t.card, borderColor: t.border }]}>
          <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '700', color: t.text }}>
            {initials(data.name)}
          </Text>
        </View>
      </View>

      <SectionHeading>Fitness</SectionHeading>
      <StatCard
        title="Strength Workout"
        icon={Heart}
        color={t.strength}
        stat={stats?.strength}
        track={trackView(prog?.tracks?.strength)}
        onPress={() => go('/add/workout')}
        onLongPress={() => go('/program')}
        outlined
      >
        {prog?.tracks?.strength ? (
          <ProgressionWindow
            track={prog.tracks.strength}
            color={t.strength}
            onPress={() => go('/add/workout')}
          />
        ) : null}
        <Tap label="Program. Today's suggested session." onPress={() => go('/program')}>
          <Caption>{"Program: today's session"}</Caption>
        </Tap>
      </StatCard>
      <StatCard
        title="Cardio Workout"
        icon={Zap}
        color={t.cardio}
        stat={stats?.cardio}
        track={trackView(prog?.tracks?.cardio)}
        onPress={() => go('/add/workout')}
      >
        {prog?.tracks?.cardio ? (
          <ProgressionWindow
            track={prog.tracks.cardio}
            color={t.cardio}
            onPress={() => go('/add/workout')}
          />
        ) : (
          <WorkoutWindow
            fraction={win.fraction}
            minutes={Math.round(win.minutes_today)}
            goal={win.goal_minutes}
            onPress={() => go('/add/workout')}
          />
        )}
      </StatCard>
      <StatCard
        title="Focus"
        icon={Eye}
        color={t.focus}
        stat={stats?.focus}
        track={trackView(prog?.tracks?.focus)}
        onPress={() => go('/add/sleep')}
      >
        {prog?.tracks?.focus ? <Caption>{FOCUS_TEXT}</Caption> : null}
      </StatCard>
      {newBests.length > 0 ? (
        <Tap
          label="New personal best. Tap to see your records."
          onPress={() => go('/records')}
          style={[styles.chip, styles.bestChip, { borderColor: t.accent }]}
        >
          <Award size={16} color={t.accent} />
          <Text style={{ fontSize: fontSizes.body, color: t.accent }}>New personal best</Text>
        </Tap>
      ) : null}
      {hints.map((h) => (
        <Caption key={h}>{h}</Caption>
      ))}

      <SectionHeading>Body & Nutrition</SectionHeading>
      <BodyRows data={shown} go={go} weightKg={weightKg} />
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), paddingTop: spacing(7), gap: spacing(1.5) },
  header: { flexDirection: 'row', alignItems: 'center', gap: spacing(2) },
  avatar: {
    width: 52,
    height: 52,
    borderRadius: 26,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  statCard: { borderWidth: 1, borderRadius: radius + 8, padding: spacing(2.5), gap: spacing(2) },
  statTap: {
    minHeight: 88,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  inset: { borderRadius: radius, padding: spacing(2), gap: spacing(1.5) },
  windowHead: { flexDirection: 'row', alignItems: 'center', gap: spacing(1) },
  bodyRow: {
    borderRadius: radius,
    padding: spacing(2),
    minHeight: 56,
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing(1.5),
  },
  rowName: { fontSize: fontSizes.body, fontWeight: '600', width: 64 },
  rowValue: { flex: 1, alignItems: 'flex-end', gap: spacing(0.5) },
  weightValue: { flexDirection: 'row', alignItems: 'center', gap: spacing(1) },
  chip: {
    borderWidth: 1,
    borderRadius: 12,
    paddingHorizontal: spacing(1),
    paddingVertical: 2,
  },
  bestChip: {
    alignSelf: 'flex-start',
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing(1),
    paddingVertical: spacing(1),
    paddingHorizontal: spacing(1.5),
  },
  dailyRing: { alignItems: 'center', gap: 2 },
  barTrack: { height: 6, width: '100%', borderRadius: 3, overflow: 'hidden' },
});
