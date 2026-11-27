import { authedRequest } from './client';
import {
  defaultSleepWindow,
  deleteSleep,
  durationMinutes,
  formatDuration,
  formatMoment,
  isValidWindow,
  listSleep,
  newSleepId,
  saveSleep,
  shiftMinutes,
  updateSleep,
} from './sleep';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

beforeEach(() => {
  mockReq.mockReset();
  mockReq.mockResolvedValue([]);
});

describe('sleep api', () => {
  it('lists without a range', async () => {
    await listSleep();
    expect(mockReq).toHaveBeenCalledWith('/sleep');
  });

  it('encodes the range so "+" in offsets survives', async () => {
    await listSleep({ from: new Date('2026-03-01T00:00:00Z'), to: new Date('2026-03-08T00:00:00Z') });
    expect(mockReq).toHaveBeenCalledWith(
      '/sleep?from=2026-03-01T00%3A00%3A00.000Z&to=2026-03-08T00%3A00%3A00.000Z',
    );
  });

  it('posts the log, including the client id', async () => {
    const input = { id: 'abc', bedtime: 'b', wake_time: 'w', quality: 4 };
    await saveSleep(input);
    expect(mockReq).toHaveBeenCalledWith('/sleep', 'POST', input);
  });

  it('updates and deletes by id', async () => {
    await updateSleep('x y', { bedtime: 'b', wake_time: 'w', quality: null });
    expect(mockReq).toHaveBeenCalledWith('/sleep/x%20y', 'PUT', {
      bedtime: 'b',
      wake_time: 'w',
      quality: null,
    });
    await deleteSleep('id1');
    expect(mockReq).toHaveBeenCalledWith('/sleep/id1', 'DELETE');
  });

  it('generates uuid-shaped ids', () => {
    const a = newSleepId();
    expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(newSleepId()).not.toBe(a);
  });
});

describe('formatDuration', () => {
  it.each([
    [460, '7 h 40 min'],
    [480, '8 h'],
    [45, '45 min'],
    [0, '0 min'],
    [61, '1 h 1 min'],
    [1440, '24 h'],
    [-5, '0 min'],
    [Number.NaN, '0 min'],
    [89.6, '1 h 30 min'],
  ])('%p -> %s', (m, text) => {
    expect(formatDuration(m)).toBe(text);
  });
});

describe('sleep window helpers', () => {
  it('prefills 23:00 -> 07:00 for last night (afternoon)', () => {
    const now = new Date(2026, 2, 10, 15, 30);
    const { bedtime, wake } = defaultSleepWindow(now);
    expect(bedtime).toEqual(new Date(2026, 2, 9, 23, 0));
    expect(wake).toEqual(new Date(2026, 2, 10, 7, 0));
    expect(durationMinutes(bedtime, wake)).toBe(480);
  });

  it('never prefills a wake time in the future', () => {
    const now = new Date(2026, 2, 10, 3, 0);
    const { bedtime, wake } = defaultSleepWindow(now);
    expect(wake).toEqual(new Date(2026, 2, 9, 7, 0));
    expect(bedtime).toEqual(new Date(2026, 2, 8, 23, 0));
  });

  it('validates windows', () => {
    const b = new Date(2026, 2, 9, 23, 0);
    expect(isValidWindow(b, shiftMinutes(b, 460))).toBe(true);
    expect(isValidWindow(b, b)).toBe(false);
    expect(isValidWindow(b, shiftMinutes(b, -10))).toBe(false);
    expect(isValidWindow(b, shiftMinutes(b, 1440))).toBe(true);
    expect(isValidWindow(b, shiftMinutes(b, 1441))).toBe(false);
  });

  it('formats moments relative to now', () => {
    const now = new Date(2026, 2, 10, 15, 30);
    expect(formatMoment(new Date(2026, 2, 10, 7, 5), now)).toBe('Today 07:05');
    expect(formatMoment(new Date(2026, 2, 9, 23, 0), now)).toBe('Yesterday 23:00');
    expect(formatMoment(new Date(2026, 2, 11, 1, 0), now)).toBe('Tomorrow 01:00');
    expect(formatMoment(new Date(2026, 1, 3, 22, 15), now)).toBe('3 Feb 22:15');
  });
});
