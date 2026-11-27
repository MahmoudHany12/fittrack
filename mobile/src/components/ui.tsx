import type { ReactNode } from 'react';
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  type TextInputProps,
} from 'react-native';

import { fontSizes, radius, spacing, useTheme } from '@/theme';

export function Title({ children }: { children: ReactNode }) {
  const t = useTheme();
  return (
    <Text style={{ fontSize: fontSizes.title, fontWeight: '600', color: t.text }}>{children}</Text>
  );
}

export function Body({ children, muted }: { children: ReactNode; muted?: boolean }) {
  const t = useTheme();
  return (
    <Text style={{ fontSize: fontSizes.body, color: muted ? t.muted : t.text }}>{children}</Text>
  );
}

export function Caption({ children, warn }: { children: ReactNode; warn?: boolean }) {
  const t = useTheme();
  return (
    <Text style={{ fontSize: fontSizes.caption, color: warn ? t.warn : t.muted }}>{children}</Text>
  );
}

export function Screen({ children }: { children: ReactNode }) {
  const t = useTheme();
  return <View style={[styles.screen, { backgroundColor: t.bg }]}>{children}</View>;
}

export function Field({
  label,
  error,
  ...input
}: TextInputProps & { label: string; error?: string }) {
  const t = useTheme();
  return (
    <View style={{ gap: spacing(0.5) }}>
      <Text style={{ fontSize: fontSizes.caption, color: t.muted }}>{label}</Text>
      <TextInput
        placeholderTextColor={t.muted}
        accessibilityLabel={label}
        style={[
          styles.input,
          { backgroundColor: t.card, color: t.text, borderColor: error ? t.warn : t.border },
        ]}
        {...input}
      />
      {error ? <Caption warn>{error}</Caption> : null}
    </View>
  );
}

export function Button({
  title,
  onPress,
  busy,
  variant = 'primary',
}: {
  title: string;
  onPress: () => void;
  busy?: boolean;
  variant?: 'primary' | 'plain';
}) {
  const t = useTheme();
  const primary = variant === 'primary';
  return (
    <Pressable
      accessibilityRole="button"
      disabled={busy}
      onPress={onPress}
      style={[
        styles.button,
        primary
          ? { backgroundColor: t.accent }
          : { borderColor: t.border, borderWidth: 1, backgroundColor: t.card },
      ]}
    >
      {busy ? (
        <ActivityIndicator color={primary ? '#FFFFFF' : t.accent} />
      ) : (
        <Text
          style={{
            fontSize: fontSizes.body,
            fontWeight: '600',
            color: primary ? '#FFFFFF' : t.text,
          }}
        >
          {title}
        </Text>
      )}
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, padding: spacing(3), gap: spacing(2), justifyContent: 'center' },
  input: {
    borderWidth: 1,
    borderRadius: radius,
    paddingHorizontal: spacing(1.5),
    paddingVertical: spacing(1.5),
    fontSize: fontSizes.body,
  },
  button: {
    borderRadius: radius,
    paddingVertical: spacing(1.5),
    alignItems: 'center',
    justifyContent: 'center',
    minHeight: 48,
  },
});
