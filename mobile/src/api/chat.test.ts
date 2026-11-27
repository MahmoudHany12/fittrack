import { authedRequest } from './client';
import {
  cleanDraft,
  deleteMessage,
  getThread,
  listConversations,
  optimisticMessage,
  sendMessage,
} from './chat';

jest.mock('./client', () => ({ authedRequest: jest.fn() }));
const mockReq = authedRequest as jest.Mock;

beforeEach(() => {
  mockReq.mockReset();
  mockReq.mockResolvedValue([]);
});

describe('chat api', () => {
  it('lists conversations', async () => {
    await listConversations();
    expect(mockReq).toHaveBeenCalledWith('/chat');
  });

  it('gets a thread with optional paging', async () => {
    await getThread('u1');
    expect(mockReq).toHaveBeenLastCalledWith('/chat/u1');
    await getThread('u1', { before: '2026-01-01T00:00:00+00:00', limit: 20 });
    expect(mockReq).toHaveBeenLastCalledWith(
      '/chat/u1?before=2026-01-01T00%3A00%3A00%2B00%3A00&limit=20',
    );
  });

  it('sends and deletes with encoded ids', async () => {
    await sendMessage('u/1', 'hi');
    expect(mockReq).toHaveBeenLastCalledWith('/chat/u%2F1', 'POST', { body: 'hi' });
    await deleteMessage('u1', 'm1');
    expect(mockReq).toHaveBeenLastCalledWith('/chat/u1/messages/m1', 'DELETE');
  });

  it('cleans drafts like the server', () => {
    expect(cleanDraft('  hi  ')).toBe('hi');
    expect(cleanDraft('a\nb')).toBe('a\nb');
    expect(cleanDraft('   ')).toBeNull();
    expect(cleanDraft('a'.repeat(2001))).toBeNull();
    expect(cleanDraft('a'.repeat(2000))).not.toBeNull();
    expect(cleanDraft('bell\u0007')).toBeNull();
  });

  it('builds an optimistic message', () => {
    const m = optimisticMessage('u1', 'yo', new Date('2026-01-01T00:00:00Z'));
    expect(m.mine).toBe(true);
    expect(m.body).toBe('yo');
    expect(m.id.startsWith('pending-')).toBe(true);
  });
});
