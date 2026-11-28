import { QueryClient } from '@tanstack/react-query';

import { setDb } from './db';
import { createFakeDb } from './fakeDb';
import { goBack, refreshAfterSave } from './hooks';
import { latestLocalWeight } from './read';
import { saveWeight } from './store';

jest.mock('./engine', () => ({ triggerSync: jest.fn(), onSynced: jest.fn() }));
jest.mock('@/api/client', () => ({ authedRequest: jest.fn() }));
jest.mock('@react-native-community/netinfo', () => ({ addEventListener: jest.fn() }));

beforeEach(() => setDb(createFakeDb()));

describe('latestLocalWeight', () => {
  it('returns the just-saved weight flagged pending', async () => {
    expect(await latestLocalWeight()).toBeNull();
    await saveWeight({ id: 'w1', kg: 80.2 });
    expect(await latestLocalWeight()).toMatchObject({ kg: 80.2, pending: true });
  });
});

describe('refreshAfterSave', () => {
  it('invalidates local, dashboard and the domain key', () => {
    const qc = new QueryClient();
    const spy = jest.spyOn(qc, 'invalidateQueries').mockResolvedValue();
    refreshAfterSave(qc, ['weight']);
    const keys = spy.mock.calls.map((c) => (c[0] as { queryKey: unknown[] }).queryKey[0]);
    expect(keys).toEqual(['local', 'dashboard', 'weight']);
  });
});

describe('goBack', () => {
  it('goes back or falls back to Home', () => {
    const r = { canGoBack: () => true, back: jest.fn(), replace: jest.fn() };
    goBack(r);
    expect(r.back).toHaveBeenCalled();
    const r2 = { canGoBack: () => false, back: jest.fn(), replace: jest.fn() };
    goBack(r2);
    expect(r2.replace).toHaveBeenCalledWith('/');
  });
});
