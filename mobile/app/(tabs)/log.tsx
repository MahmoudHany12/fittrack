import { Link } from 'expo-router';

import { Card } from '@/components/Card';
import { Body, Screen, Title } from '@/components/ui';

const ITEMS = [
  { href: '/add/workout', label: 'Workout' },
  { href: '/add/food', label: 'Food' },
  { href: '/add/sleep', label: 'Sleep' },
  { href: '/add/weight', label: 'Weight' },
] as const;

export default function Log() {
  return (
    <Screen>
      <Title>Log</Title>
      {ITEMS.map((i) => (
        <Link key={i.href} href={i.href} asChild>
          <Card onPress={() => {}}>
            <Body>{i.label}</Body>
          </Card>
        </Link>
      ))}
    </Screen>
  );
}
