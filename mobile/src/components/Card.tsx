import type { ReactNode } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';

import { radius, spacing, useTheme } from '@/theme';

/** Flat card: 1px border, radius 16, no shadow. Tappable when onPress is given. */
export function Card({ children, onPress }: { children: ReactNode; onPress?: () => void }) {
  const t = useTheme();
  const style = [styles.card, { backgroundColor: t.card, borderColor: t.border }];
  if (onPress) {
    return (
      <Pressable accessibilityRole="button" onPress={onPress} style={style}>
        {children}
      </Pressable>
    );
  }
  return <View style={style}>{children}</View>;
}

const styles = StyleSheet.create({
  card: {
    borderWidth: 1,
    borderRadius: radius,
    padding: spacing(2),
  },
});
