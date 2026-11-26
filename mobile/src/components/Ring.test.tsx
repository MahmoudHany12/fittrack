import { Dumbbell } from 'lucide-react-native';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { ReactElement } from 'react';

import { Card } from './Card';
import { Ring } from './Ring';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

async function render(el: ReactElement): Promise<ReactTestRenderer> {
  let r!: ReactTestRenderer;
  await act(async () => {
    r = create(el);
  });
  return r;
}

describe('Ring', () => {
  it('renders with clamped progress', async () => {
    const r = await render(<Ring progress={1.7} icon={Dumbbell} color="#F2A65A" />);
    const bar = r.root.findByProps({ accessibilityRole: 'progressbar' });
    expect(bar.props.accessibilityValue.now).toBe(100);
  });

  it('treats NaN as zero', async () => {
    const r = await render(<Ring progress={Number.NaN} icon={Dumbbell} color="#F2A65A" />);
    const bar = r.root.findByProps({ accessibilityRole: 'progressbar' });
    expect(bar.props.accessibilityValue.now).toBe(0);
  });
});

describe('Card', () => {
  it('renders', async () => {
    const r = await render(<Card>{null}</Card>);
    expect(r.toJSON()).not.toBeNull();
  });
});
