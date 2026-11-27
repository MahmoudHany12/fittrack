import { QueryClient, useQueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Stack, useRouter, useSegments } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';

import { AuthProvider, useAuth } from '@/auth/AuthProvider';
import { onSynced, startEngine, stopEngine } from '@/sync/engine';

/** Runs the offline-sync engine while signed in and refreshes screens after each sync. */
function SyncEngine() {
  const { status } = useAuth();
  const queryClient = useQueryClient();
  useEffect(() => {
    if (status !== 'signedIn') return;
    const off = onSynced((r) => {
      void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      if (r.pushed > 0 || r.pulled > 0) {
        for (const key of ['workouts', 'sleep', 'weight', 'food']) {
          void queryClient.invalidateQueries({ queryKey: [key] });
        }
      }
    });
    startEngine();
    return () => {
      off();
      stopEngine();
    };
  }, [status, queryClient]);
  return null;
}

function Gate() {
  const { status } = useAuth();
  const segments = useSegments();
  const router = useRouter();

  useEffect(() => {
    if (status === 'loading') return;
    const inAuth = segments[0] === '(auth)';
    if (status === 'signedOut' && !inAuth) router.replace('/login');
    else if (status === 'signedIn' && inAuth) router.replace('/');
  }, [status, segments, router]);

  if (status === 'loading') return null;
  return (
    <Stack screenOptions={{ headerShown: false }}>
      <Stack.Screen name="(auth)" />
      <Stack.Screen name="(tabs)" />
    </Stack>
  );
}

export default function RootLayout() {
  const [client] = useState(() => new QueryClient());
  return (
    <QueryClientProvider client={client}>
      <AuthProvider>
        <StatusBar style="auto" />
        <SyncEngine />
        <Gate />
      </AuthProvider>
    </QueryClientProvider>
  );
}
