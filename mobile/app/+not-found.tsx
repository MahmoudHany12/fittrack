import { Link } from 'expo-router';

import { Body, Screen } from '@/components/ui';

export default function NotFound() {
  return (
    <Screen>
      <Body>That page does not exist.</Body>
      <Link href="/">
        <Body muted>Go home</Body>
      </Link>
    </Screen>
  );
}
