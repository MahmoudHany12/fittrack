import { useAuth } from '@/auth/AuthProvider';
import { Button, Screen, Title } from '@/components/ui';

export default function Settings() {
  const { signOut } = useAuth();
  return (
    <Screen>
      <Title>Settings</Title>
      <Button title="Sign out" variant="plain" onPress={() => void signOut()} />
    </Screen>
  );
}
