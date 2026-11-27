import { authedRequest } from './client';
import {
  acceptInvite,
  createInvite,
  getFriendSummary,
  inviteMessage,
  labelFor,
  listFriends,
  parseInviteCode,
  removeFriend,
  setShare,
  summaryRows,
} from './friends';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

beforeEach(() => {
  mockReq.mockReset();
  mockReq.mockResolvedValue({});
});

describe('friends api', () => {
  it('lists friends and creates invites', async () => {
    await listFriends();
    expect(mockReq).toHaveBeenCalledWith('/friends');
    await createInvite();
    expect(mockReq).toHaveBeenCalledWith('/friends/invites', 'POST');
  });

  it('accepts by code, encoding the path segment', async () => {
    await acceptInvite('ABCDEFGHJK');
    expect(mockReq).toHaveBeenCalledWith('/friends/invites/ABCDEFGHJK/accept', 'POST');
    await acceptInvite('a/b');
    expect(mockReq).toHaveBeenLastCalledWith('/friends/invites/a%2Fb/accept', 'POST');
  });

  it('sends the share consent flag', async () => {
    await setShare('u1', true);
    expect(mockReq).toHaveBeenCalledWith('/friends/u1/share', 'PUT', { enabled: true });
    await setShare('u1', false);
    expect(mockReq).toHaveBeenLastCalledWith('/friends/u1/share', 'PUT', { enabled: false });
  });

  it('removes a friend and reads their summary', async () => {
    await removeFriend('u 1');
    expect(mockReq).toHaveBeenCalledWith('/friends/u%201', 'DELETE');
    await getFriendSummary('u1');
    expect(mockReq).toHaveBeenLastCalledWith('/friends/u1/summary');
  });
});

describe('parseInviteCode', () => {
  it.each([
    ['ABCDEFGHJK', 'ABCDEFGHJK'],
    ['  abcdefghjk ', 'ABCDEFGHJK'],
    ['abcde-fghjk', 'ABCDEFGHJK'],
    ['fittrack://invite/abcdefghjk', 'ABCDEFGHJK'],
    ['FITTRACK://invite/ABCDEFGHJK?x=1', 'ABCDEFGHJK'],
  ])('%p -> %p', (input, code) => {
    expect(parseInviteCode(input)).toBe(code);
  });

  it.each(['', 'short', 'ABCDEFGHJ0', 'ABCDEFGHIJ', 'ABCDEFGHJKL', 'https://evil.test/ABCDEFGHJK'])(
    'rejects %p',
    (input) => {
      expect(parseInviteCode(input)).toBeNull();
    },
  );
});

describe('helpers', () => {
  it('builds the share message with link and code', () => {
    const msg = inviteMessage({
      code: 'ABCDEFGHJK',
      link: 'fittrack://invite/ABCDEFGHJK',
      expires_at: 'x',
    });
    expect(msg).toContain('fittrack://invite/ABCDEFGHJK');
    expect(msg).toContain('ABCDEFGHJK');
  });

  it('labels keys readably', () => {
    expect(labelFor('workout_minutes')).toBe('Workout minutes');
  });

  it('shows only primitive summary values', () => {
    expect(
      summaryRows({
        week_start: '2026-03-02',
        placeholder: true,
        user_id: 'u',
        workouts: 3,
        nested: { a: 1 },
        nothing: null,
      }),
    ).toEqual([
      { label: 'Week start', value: '2026-03-02' },
      { label: 'Workouts', value: '3' },
    ]);
  });
});
