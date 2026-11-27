import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Text } from 'react-native';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import { ApiError } from '@/api/client';
import { acceptInvite } from '@/api/friends';

import InviteScreen from '../../app/invite/[code]';

jest.mock('@/api/friends', () => {
  const actual = jest.requireActual('@/api/friends');
  return { ...actual, acceptInvite: jest.fn() };
});
jest.mock('expo-router', () => ({
  useLocalSearchParams: jest.fn(() => ({ code: 'ABCDEFGHJK' })),
  useRouter: jest.fn(),
}));

const mockAccept = acceptInvite as jest.Mock;
const replace = jest.fn();

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Flattens React children (strings, numbers, nested arrays) into plain text. */
function childText(c: unknown): string {
  if (typeof c === 'string' || typeof c === 'number') return String(c);
  if (Array.isArray(c)) return c.map(childText).join('');
  return '';
}

function textOf(node: ReactTestRenderer['root']): string {
  return node
    .findAllByType(Text)
    .map((t) => childText(t.props.children))
    .join('|');
}

function screenText(r: ReactTestRenderer): string {
  return textOf(r.root);
}

async function render(): Promise<ReactTestRenderer> {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  let r!: ReactTestRenderer;
  await act(async () => {
    r = create(
      <QueryClientProvider client={client}>
        <InviteScreen />
      </QueryClientProvider>,
    );
  });
  return r;
}

async function pressButton(r: ReactTestRenderer, label: string): Promise<void> {
  const button = r.root.findAll(
    (n) =>
      n.props.accessibilityRole === 'button' &&
      typeof n.props.onPress === 'function' &&
      textOf(n).includes(label),
  )[0];
  if (!button) throw new Error(`no button "${label}"`);
  await act(async () => {
    button.props.onPress();
    // Let the mutation settle (resolve or reject) before asserting.
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  mockAccept.mockReset();
  replace.mockReset();
  (useRouter as jest.Mock).mockReturnValue({ replace, back: jest.fn() });
});

describe('invite screen', () => {
  it('asks to add the code as a friend', async () => {
    const r = await render();
    expect(screenText(r)).toContain('Add ABCDEFGHJK as a friend?');
  });

  it('accepts the invite, then returns to the friends tab', async () => {
    mockAccept.mockResolvedValue({ friend_id: 'u2' });
    const r = await render();
    await pressButton(r, 'Accept invite');
    expect(mockAccept).toHaveBeenCalledWith('ABCDEFGHJK');
    expect(replace).toHaveBeenCalledWith('/(tabs)/friends');
  });

  it.each([
    [404, 'That invite has expired or was already used.'],
    [409, 'You are already friends.'],
    [400, "That's your own invite."],
    [500, 'Could not accept the invite. Try again.'],
  ])('shows a plain error for status %i', async (status, message) => {
    mockAccept.mockRejectedValue(new ApiError(status, 'x', 'raw server text'));
    const r = await render();
    await pressButton(r, 'Accept invite');
    expect(screenText(r)).toContain(message);
    expect(replace).not.toHaveBeenCalled();
  });

  it('Not now leaves without accepting', async () => {
    const r = await render();
    await pressButton(r, 'Not now');
    expect(mockAccept).not.toHaveBeenCalled();
    expect(replace).toHaveBeenCalledWith('/(tabs)/friends');
  });
});
