import type { WindowTrack } from '@/api/progression';

import { formatLeft, pickHints, ringText, trackView, windowFraction, windowView } from './progression';

const NOW = Date.parse('2026-06-15T10:00:00.000Z');
const h = (n: number) => new Date(NOW + n * 3600_000).toISOString();

const track = (p: Partial<WindowTrack>): WindowTrack => ({
  level: { level: 2, progress: 0.5, value: 40 },
  state: 'open',
  opens_at: null,
  closes_at: null,
  fraction: 0.5,
  ...p,
});

describe('formatLeft', () => {
  it('uses minutes under an hour and hours above', () => {
    expect(formatLeft(45 * 60000)).toBe('45 min');
    expect(formatLeft(31 * 3600_000)).toBe('31 h');
    expect(formatLeft(0)).toBe('1 min');
  });
});

describe('windowView', () => {
  it('open: drains with the time left', () => {
    const v = windowView(track({ state: 'open', closes_at: h(31), fraction: 0.65 }), NOW);
    expect(v.text).toBe('Window open · 31 h left');
    expect(v.fraction).toBe(0.65);
    expect(v.warn).toBe(false);
  });

  it('recovering: fills toward opening, with protein bonus chip flag', () => {
    const v = windowView(
      track({ state: 'recovering', opens_at: h(6), fraction: 0.7, protein_bonus: true }),
      NOW,
    );
    expect(v.text).toBe('Recovering · opens in 6 h');
    expect(v.proteinBonus).toBe(true);
    expect(v.fraction).toBe(0.7);
  });

  it('needs_rest and missed have plain words; missed is amber', () => {
    expect(windowView(track({ state: 'needs_rest' }), NOW)).toMatchObject({
      text: 'Sleep to open your next window',
      warn: false,
      fraction: 0,
    });
    expect(windowView(track({ state: 'missed' }), NOW)).toMatchObject({
      text: 'Window missed — train to get back on track',
      warn: true,
    });
  });

  it('copes with missing times', () => {
    expect(windowView(track({ state: 'open' }), NOW).text).toBe('Window open');
    expect(windowView(track({ state: 'recovering', opens_at: 'bad' }), NOW).text).toBe('Recovering');
  });
});

describe('windowFraction', () => {
  it('clamps and defaults', () => {
    expect(windowFraction({ state: 'open', fraction: 3 })).toBe(1);
    expect(windowFraction({ state: 'recovering', fraction: -1 })).toBe(0);
    expect(windowFraction({ state: 'open', fraction: NaN })).toBe(1);
    expect(windowFraction({ state: 'recovering', fraction: NaN })).toBe(0);
  });
});

describe('trackView, ringText, pickHints', () => {
  it('trackView clamps level and returns null without a track', () => {
    expect(trackView(undefined)).toBeNull();
    expect(trackView({ level: 9, progress: 2, value: 0 })).toEqual({ level: 4, progress: 1 });
  });

  it('ringText shows minutes or done', () => {
    expect(ringText({ minutes: 12, goal: 30, closed: false })).toEqual({
      label: '12/30 min',
      fraction: 0.4,
    });
    expect(ringText({ minutes: 45, goal: 30, closed: true })).toEqual({
      label: 'Done for today',
      fraction: 1,
    });
  });

  it('pickHints keeps at most two non-empty hints', () => {
    expect(pickHints(['a', ' ', 'b', 'c'])).toEqual(['a', 'b']);
    expect(pickHints(undefined)).toEqual([]);
  });
});
