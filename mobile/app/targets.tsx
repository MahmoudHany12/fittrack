import { useQuery } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { View } from 'react-native';

import {
  bmrSourceText,
  calorieSourceText,
  getTargets,
  missingHint,
  targetsKeys,
  tdeeSourceText,
} from '@/api/targets';
import { Card } from '@/components/Card';
import { ScrollScreen, SectionTitle } from '@/components/forms';
import { Body, Button, Caption, Title } from '@/components/ui';

function Row({ label, value }: { label: string; value: string }) {
  return (
    <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
      <Body muted>{label}</Body>
      <Body>{value}</Body>
    </View>
  );
}

const g = (n: number | null) => (n === null ? '-' : `${n} g`);
const kcal = (n: number | null) => (n === null ? '-' : `${n} kcal`);

export default function Targets() {
  const router = useRouter();
  const q = useQuery({ queryKey: targetsKeys.all, queryFn: getTargets });
  const t = q.data;

  if (!t) {
    return (
      <ScrollScreen>
        <Title>Your daily targets</Title>
        {q.isError ? <Caption warn>Could not load your targets.</Caption> : <Caption>Loading...</Caption>}
      </ScrollScreen>
    );
  }

  return (
    <ScrollScreen>
      <Title>Your daily targets</Title>
      <Card>
        <Caption>Calories</Caption>
        <Title>{`${t.calorie_target} kcal`}</Title>
        <Caption>{calorieSourceText(t.calorie_target_source)}</Caption>
      </Card>
      <Card>
        <Row label="Protein" value={g(t.protein_g)} />
        <Row label="Fat" value={g(t.fat_g)} />
        <Row label="Carbs" value={g(t.carbs_g)} />
      </Card>
      <SectionTitle>Where this comes from</SectionTitle>
      <Card>
        <Row label="BMR (energy at rest)" value={kcal(t.bmr)} />
        <Caption>{bmrSourceText(t.bmr_source)}</Caption>
      </Card>
      <Card>
        <Row label="TDEE (energy you burn a day)" value={kcal(t.tdee)} />
        <Caption>{tdeeSourceText(t.tdee_source)}</Caption>
      </Card>
      {t.missing.length > 0 ? (
        <View style={{ gap: 8 }}>
          <SectionTitle>Improve accuracy</SectionTitle>
          {t.missing.map((m) => {
            const h = missingHint(m);
            return <Button key={m} title={h.text} variant="plain" onPress={() => router.push(h.to)} />;
          })}
        </View>
      ) : null}
    </ScrollScreen>
  );
}
