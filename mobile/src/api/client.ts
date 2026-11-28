import Constants from 'expo-constants';

import { clearTokens, loadTokens, saveTokens } from './tokens';

/**
 * API base URL. EXPO_PUBLIC_API_URL wins (production builds). In development, when it is
 * not set, use the machine that serves the JS bundle (Expo's hostUri, e.g. "10.0.0.5:8081")
 * on port 8080, so the app keeps working when the computer's Wi-Fi IP changes.
 */
export function resolveApiUrl(envUrl: string | undefined, hostUri: string | undefined | null): string {
  if (envUrl && envUrl.trim()) return envUrl.trim().replace(/\/+$/, '');
  const host = hostUri?.split(':')[0];
  if (host) return `http://${host}:8080`;
  return 'http://localhost:8080';
}

export const API_URL = resolveApiUrl(process.env.EXPO_PUBLIC_API_URL, Constants.expoConfig?.hostUri);

export type TokenResponse = {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
};

export type Me = { id?: string; email: string; [key: string]: unknown };

export class ApiError extends Error {
  status: number;
  code: string;
  fields?: Record<string, string>;

  constructor(status: number, code: string, message: string, fields?: Record<string, string>) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.fields = fields;
  }
}

let onSignedOut: (() => void) | null = null;
/** The auth provider registers a callback so a failed refresh signs the user out. */
export function setSignedOutHandler(fn: (() => void) | null): void {
  onSignedOut = fn;
}

async function parseError(res: Response): Promise<ApiError> {
  try {
    const body = await res.json();
    const e = body?.error;
    if (e) {
      return new ApiError(res.status, e.code ?? 'error', e.message ?? 'Something went wrong.', e.fields);
    }
  } catch {
    // fall through to the generic message
  }
  return new ApiError(res.status, 'error', 'Something went wrong. Please try again.');
}

async function rawRequest(
  path: string,
  method: string,
  body?: unknown,
  accessToken?: string,
): Promise<Response> {
  const headers: Record<string, string> = { Accept: 'application/json' };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (accessToken) headers.Authorization = `Bearer ${accessToken}`;
  try {
    return await fetch(`${API_URL}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, 'network', 'Cannot reach the server. Check your connection.');
  }
}

async function toJson<T>(res: Response): Promise<T> {
  if (!res.ok) throw await parseError(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

let refreshing: Promise<string | null> | null = null;

/** Single-flight refresh: concurrent 401s share one request. Resolves to the new access token or null. */
export function refreshAccessToken(): Promise<string | null> {
  if (!refreshing) {
    refreshing = (async () => {
      try {
        const tokens = await loadTokens();
        if (!tokens) return null;
        const res = await rawRequest('/auth/refresh', 'POST', { refresh_token: tokens.refreshToken });
        if (!res.ok) return null;
        const t = (await res.json()) as TokenResponse;
        await saveTokens({ accessToken: t.access_token, refreshToken: t.refresh_token });
        return t.access_token;
      } catch {
        return null;
      } finally {
        refreshing = null;
      }
    })();
  }
  return refreshing;
}

/** Authenticated request: on 401 refresh once and retry once, else sign out. */
export async function authedRequest<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const tokens = await loadTokens();
  let res = await rawRequest(path, method, body, tokens?.accessToken);
  if (res.status === 401) {
    const fresh = await refreshAccessToken();
    if (fresh) res = await rawRequest(path, method, body, fresh);
    if (!fresh || res.status === 401) {
      await clearTokens();
      onSignedOut?.();
      throw new ApiError(401, 'unauthorized', 'Your session has ended. Please sign in again.');
    }
  }
  return toJson<T>(res);
}

async function storeTokens(t: TokenResponse): Promise<void> {
  await saveTokens({ accessToken: t.access_token, refreshToken: t.refresh_token });
}

export async function register(email: string, password: string): Promise<void> {
  const res = await rawRequest('/auth/register', 'POST', { email, password });
  await storeTokens(await toJson<TokenResponse>(res));
}

export async function login(email: string, password: string): Promise<void> {
  const res = await rawRequest('/auth/login', 'POST', { email, password });
  await storeTokens(await toJson<TokenResponse>(res));
}

export async function logout(): Promise<void> {
  const tokens = await loadTokens();
  try {
    if (tokens) await rawRequest('/auth/logout', 'POST', { refresh_token: tokens.refreshToken });
  } catch {
    // signing out locally still has to work offline
  } finally {
    await clearTokens();
  }
}

export function getMe(): Promise<Me> {
  return authedRequest<Me>('/me');
}
