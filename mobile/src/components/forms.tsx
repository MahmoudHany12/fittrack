import type { ReactNode } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { fontSizes, radius, spacing, useTheme } from '@/theme';

/** Scrollable screen for long forms (the plain Screen does not scroll). */
export function ScrollScreen({ children }: { children: ReactNode }) {
  const t = useTheme();
  return (
    <ScrollView
      style={{ backgroundColor: t.bg }}
      contentContainerStyle={styles.content}
      keyboardShouldPersistTaps="handled"
    >
      {children}
    </ScrollView>
  );
}

export function SectionTitle({ children }: { children: ReactNode }) {
  const t = useTheme();
  return (
    <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text, marginTop: spacing(1) }}>
      {children}
    </Text>
  );
}

export type Option<T extends string | number> = { value: T; label: string };

/** Flat segmented buttons; tapping the selected one again clears it when `clearable`. */
export function Segmented<T extends string | number>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: Option<T>[];
  value: T | null | undefined;
  onChange: (v: T) => void;
}) {
  const t = useTheme();
  return (
    <View style={{ gap: spacing(0.5) }}>
      <Text style={{ fontSize: fontSizes.caption, color: t.muted }}>{label}</Text>
      <View style={styles.row}>
        {options.map((o) => {
          const on = o.value === value;
          return (
            <Pressable
              key={String(o.value)}
              accessibilityRole="button"
              accessibilityLabel={`${label}: ${o.label}`}
              accessibilityState={{ selected: on }}
              onPress={() => onChange(o.value)}
              style={[
                styles.seg,
                { borderColor: on ? t.accent : t.border, backgroundColor: on ? t.accent : t.card },
              ]}
            >
              <Text style={{ fontSize: fontSizes.body, fontWeight: '600', color: on ? '#0E1110' : t.text }}>
                {o.label}
              </Text>
            </Pressable>
          );
        })}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), gap: spacing(2), paddingBottom: spacing(6) },
  row: { flexDirection: 'row', flexWrap: 'wrap', gap: spacing(1) },
  seg: {
    borderWidth: 1,
    borderRadius: radius,
    paddingVertical: spacing(1),
    paddingHorizontal: spacing(2),
    minHeight: 40,
    justifyContent: 'center',
  },
});
