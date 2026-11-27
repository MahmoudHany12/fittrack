import { authedRequest } from './client';
import { getConsents, importHealth, putConsents } from './health';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

beforeEach(() => {
  mockReq.mockReset();
  mockReq.mockResolvedValue({});
});

describe('health api', () => {
  it('gets consents', async () => {
    await getConsents();
    expect(mockReq).toHaveBeenCalledWith('/health/consents');
  });

  it('puts all three consents', async () => {
    const c = { steps: true, workouts: false, sleep: true };
    await putConsents(c);
    expect(mockReq).toHaveBeenCalledWith('/health/consents', 'PUT', c);
  });

  it('posts the import body', async () => {
    const body = { steps: [{ external_id: 'steps-2026-03-01', date: '2026-03-01', count: 10 }] };
    await importHealth(body);
    expect(mockReq).toHaveBeenCalledWith('/health/import', 'POST', body);
  });
});
