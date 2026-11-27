import { ApiError, getMe, login, setSignedOutHandler } from './client';

// Variables used inside a jest.mock factory must be prefixed with "mock".
const mockStore: Record<string, string> = {};
const store = mockStore;

jest.mock('expo-secure-store', () => ({
  getItemAsync: jest.fn(async (k: string) => mockStore[k] ?? null),
  setItemAsync: jest.fn(async (k: string, v: string) => {
    mockStore[k] = v;
  }),
  deleteItemAsync: jest.fn(async (k: string) => {
    delete mockStore[k];
  }),
}));

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

const tokenBody = (a: string, r: string) => ({
  access_token: a,
  refresh_token: r,
  token_type: 'Bearer',
  expires_in: 900,
});

let fetchMock: jest.Mock;

beforeEach(() => {
  for (const k of Object.keys(store)) delete store[k];
  store['fittrack.access_token'] = 'old-access';
  store['fittrack.refresh_token'] = 'old-refresh';
  fetchMock = jest.fn();
  global.fetch = fetchMock as unknown as typeof fetch;
  setSignedOutHandler(null);
});

function auth(call: unknown[]): string | undefined {
  return (call[1] as { headers: Record<string, string> }).headers.Authorization;
}

describe('refresh on 401', () => {
  it('refreshes once and retries with the new token', async () => {
    fetchMock
      .mockResolvedValueOnce(json(401, { error: { code: 'unauthorized', message: 'expired' } }))
      .mockResolvedValueOnce(json(200, tokenBody('new-access', 'new-refresh')))
      .mockResolvedValueOnce(json(200, { email: 'a@b.co' }));

    await expect(getMe()).resolves.toEqual({ email: 'a@b.co' });

    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(fetchMock.mock.calls[1][0]).toMatch(/\/auth\/refresh$/);
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ refresh_token: 'old-refresh' });
    expect(auth(fetchMock.mock.calls[0])).toBe('Bearer old-access');
    expect(auth(fetchMock.mock.calls[2])).toBe('Bearer new-access');
    expect(store['fittrack.refresh_token']).toBe('new-refresh');
  });

  it('shares a single refresh between concurrent requests', async () => {
    let refreshCalls = 0;
    fetchMock.mockImplementation(async (url: string, init: { headers: Record<string, string> }) => {
      if (url.endsWith('/auth/refresh')) {
        refreshCalls += 1;
        return json(200, tokenBody('new-access', 'new-refresh'));
      }
      return init.headers.Authorization === 'Bearer new-access'
        ? json(200, { email: 'a@b.co' })
        : json(401, { error: { code: 'unauthorized', message: 'expired' } });
    });

    await Promise.all([getMe(), getMe()]);
    expect(refreshCalls).toBe(1);
  });

  it('signs out when the refresh fails', async () => {
    const onOut = jest.fn();
    setSignedOutHandler(onOut);
    fetchMock
      .mockResolvedValueOnce(json(401, { error: { code: 'unauthorized', message: 'expired' } }))
      .mockResolvedValueOnce(json(401, { error: { code: 'unauthorized', message: 'bad refresh' } }));

    await expect(getMe()).rejects.toMatchObject({ status: 401 });
    expect(onOut).toHaveBeenCalledTimes(1);
    expect(store['fittrack.access_token']).toBeUndefined();
    expect(store['fittrack.refresh_token']).toBeUndefined();
  });

  it('retries only once if the retry also returns 401', async () => {
    const onOut = jest.fn();
    setSignedOutHandler(onOut);
    fetchMock
      .mockResolvedValueOnce(json(401, { error: { code: 'unauthorized', message: 'x' } }))
      .mockResolvedValueOnce(json(200, tokenBody('new-access', 'new-refresh')))
      .mockResolvedValueOnce(json(401, { error: { code: 'unauthorized', message: 'x' } }));

    await expect(getMe()).rejects.toBeInstanceOf(ApiError);
    expect(fetchMock).toHaveBeenCalledTimes(3);
    expect(onOut).toHaveBeenCalledTimes(1);
  });
});

describe('login', () => {
  it('stores tokens on success', async () => {
    fetchMock.mockResolvedValueOnce(json(200, tokenBody('a1', 'r1')));
    await login('a@b.co', 'password-1234');
    expect(store['fittrack.access_token']).toBe('a1');
    expect(store['fittrack.refresh_token']).toBe('r1');
  });

  it('surfaces server field errors', async () => {
    fetchMock.mockResolvedValueOnce(
      json(422, {
        error: { code: 'validation', message: 'Invalid input', fields: { password: 'too short' } },
      }),
    );
    await expect(login('a@b.co', 'x')).rejects.toMatchObject({
      status: 422,
      fields: { password: 'too short' },
    });
  });
});
