import { Link } from 'expo-router';
import { View } from 'react-native';

import { useAuth } from '@/auth/AuthProvider';
import { useAuthForm } from '@/auth/useAuthForm';
import { Body, Button, Caption, Field, Screen, Title } from '@/components/ui';
import { MIN_PASSWORD_LENGTH, validateRegister } from '@/validation';

export default function Register() {
  const { signUp } = useAuth();
  const f = useAuthForm(validateRegister, signUp);
  return (
    <Screen>
      <Title>Create account</Title>
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
        label={`Password (at least ${MIN_PASSWORD_LENGTH} characters)`}
        value={f.password}
        onChangeText={f.setPassword}
        error={f.errors.password}
        secureTextEntry
        autoComplete="new-password"
      />
      {f.formError ? <Caption warn>{f.formError}</Caption> : null}
      <Button title="Create account" onPress={f.onSubmit} busy={f.busy} />
      <View style={{ alignItems: 'center' }}>
        <Link href="/login">
          <Body muted>Already have an account? Sign in</Body>
        </Link>
      </View>
    </Screen>
  );
}
