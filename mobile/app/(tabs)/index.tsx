import { useQuery } from '@tanstack/react-query';
import { Activity, Brain, Dumbbell } from 'lucide-react-native';
import { ScrollView, StyleSheet, Text, View } from 'react-native';

import { getMe } from '@/api/client';
import { Card } from '@/components/Card';
import { Ring } from '@/components/Ring';
import { Body, Caption, Title } from '@/components/ui';
import { fontSizes, spacing, useTheme } from '@/theme';

function SectionHeading({ children }: { children: string }) {
  const t = useTheme();
  return (
    <Text style={{ fontSize: fontSizes.body, fontWeight: '600', color: t.muted, marginTop: spacing(1) }}>
      {children}
    </Text>
  );
}

export default function Home() {
  const t = useTheme();
  const { data, isError } = useQuery({ queryKey: ['me'], queryFn: getMe });
  const name = data?.email.split('@')[0] ?? '';

  const stats = [
    { title: 'Strength', icon: Dumbbell, color: t.strength },
    { title: 'Cardio', icon: Activity, color: t.cardio },
    { title: 'Focus', icon: Brain, color: t.focus },
  ];

  return (
    <ScrollView style={{ backgroundColor: t.bg }} contentContainerStyle={styles.content}>
      <View>
        <Title>{name || 'Home'}</Title>
        <Text style={{ fontSize: fontSizes.cardTitle, color: t.muted }}>No data yet</Text>
        {data ? <Caption>Signed in as {data.email}</Caption> : null}
        {isError ? <Caption warn>Could not load your account. Try again in a moment.</Caption> : null}
      </View>

      <SectionHeading>Fitness</SectionHeading>
      {stats.map((s) => (
        <Card key={s.title}>
          <View style={styles.row}>
            <View>
              <Text style={{ fontSize: fontSizes.cardTitle, fontWeight: '600', color: t.text }}>
                {s.title}
              </Text>
              <Caption>Level 1/4</Caption>
            </View>
            <Ring progress={0} icon={s.icon} color={s.color} />
          </View>
        </Card>
      ))}

      <SectionHeading>Body & Nutrition</SectionHeading>
      <Card>
        <Body muted>Sleep, diet and weight will show here.</Body>
      </Card>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  content: { padding: spacing(3), paddingTop: spacing(7), gap: spacing(1.5) },
  row: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
});
