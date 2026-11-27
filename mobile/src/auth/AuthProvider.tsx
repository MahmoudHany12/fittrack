import { useQueryClient } from '@tanstack/react-query';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import * as api from '@/api/client';
import { loadTokens } from '@/api/tokens';

type Status = 'loading' | 'signedOut' | 'signedIn';

type AuthValue = {
  status: Status;
  signIn: (email: string, password: string) => Promise<void>;
  signUp: (email: string, password: string) => Promise<void>;
  signOut: () => Promise<void>;
};

const AuthContext = createContext<AuthValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<Status>('loading');
  const queryClient = useQueryClient();

  useEffect(() => {
    api.setSignedOutHandler(() => {
      queryClient.clear();
      setStatus('signedOut');
    });
    let alive = true;
    loadTokens()
      .then((t) => alive && setStatus(t ? 'signedIn' : 'signedOut'))
      .catch(() => alive && setStatus('signedOut'));
    return () => {
      alive = false;
      api.setSignedOutHandler(null);
    };
  }, [queryClient]);

  const signIn = useCallback(async (email: string, password: string) => {
    await api.login(email, password);
    setStatus('signedIn');
  }, []);

  const signUp = useCallback(async (email: string, password: string) => {
    await api.register(email, password);
    setStatus('signedIn');
  }, []);

  const signOut = useCallback(async () => {
    await api.logout();
    queryClient.clear();
    setStatus('signedOut');
  }, [queryClient]);

  const value = useMemo(
    () => ({ status, signIn, signUp, signOut }),
    [status, signIn, signUp, signOut],
  );
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used inside AuthProvider');
  return ctx;
}
