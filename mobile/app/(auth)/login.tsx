import { Link } from 'expo-router';
import { View } from 'react-native';

import { useAuth } from '@/auth/AuthProvider';
import { useAuthForm } from '@/auth/useAuthForm';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';
import { validateLogin } from '@/validation';

export default function Login() {
  const { signIn } = useAuth();
  const f = useAuthForm(validateLogin, signIn);
  return (
    <Screen>
      <Title>FitTrack</Title>
      <Body muted>Sign in to continue.</Body>
      <Field
        label="Email"
        value={f.email}
        onChangeText={f.setEmail}
        error={f.errors.email}
        autoCapitalize="none"
        autoComplete="email"
        keyboardType="email-address"
      />
      <Field
        label="Password"
        value={f.password}
        onChangeText={f.setPassword}
        error={f.errors.password}
        secureTextEntry
        autoComplete="current-password"
      />
      {f.formError ? <Caption warn>{f.formError}</Caption> : null}
      <Button title="Sign in" onPress={f.onSubmit} busy={f.busy} />
      <View style={{ alignItems: 'center' }}>
        <Link href="/register">
          <Body muted>New here? Create an account</Body>
        </Link>
      </View>
    </Screen>
  );
}
