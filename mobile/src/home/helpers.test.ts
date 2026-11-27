import { palettes } from '@/theme';

import {
  dietColor,
  formatHours,
  formatKg,
  formatNumber,
  kcalFraction,
  levelFromValue,
  levelProgress,
  statusColor,
  statView,
  trendArrow,
  trendWord,
} from './helpers';

const p = palettes.dark;

describe('levelFromValue', () => {
  it.each([
    [0, 1],
    [24, 1],
    [25, 2],
    [49, 2],
    [50, 3],
    [74, 3],
    [75, 4],
    [100, 4],
    [250, 4],
    [-5, 1],
  ])('%p -> level %p', (v, l) => expect(levelFromValue(v)).toBe(l));
  it('handles null, undefined and NaN', () => {
    expect(levelFromValue(null)).toBe(1);
    expect(levelFromValue(undefined)).toBe(1);
    expect(levelFromValue(Number.NaN)).toBe(1);
  });
});

describe('levelProgress', () => {
  it('is progress inside the level', () => {
    expect(levelProgress(0)).toBe(0);
    expect(levelProgress(40)).toBeCloseTo(0.6);
    expect(levelProgress(25)).toBe(0);
    expect(levelProgress(99)).toBeCloseTo(0.96);
  });
  it('is full at 100 and beyond, empty for bad input', () => {
    expect(levelProgress(100)).toBe(1);
    expect(levelProgress(130)).toBe(1);
    expect(levelProgress(-3)).toBe(0);
    expect(levelProgress(null)).toBe(0);
  });
});

describe('statView', () => {
  it('prefers server level/progress', () => {
    expect(statView({ value: 40, level: 3, progress: 0.2 })).toEqual({ level: 3, progress: 0.2 });
  });
  it('derives from value when missing', () => {
    expect(statView({ value: 40, level: null, progress: null }).level).toBe(2);
  });
  it('clamps and handles null', () => {
    expect(statView({ value: 1, level: 9, progress: 4 })).toEqual({ level: 4, progress: 1 });
    expect(statView(null)).toEqual({ level: 1, progress: 0 });
  });
});

describe('kcalFraction', () => {
  it('computes and clamps', () => {
    expect(kcalFraction(1100, 2200)).toBe(0.5);
    expect(kcalFraction(3000, 2200)).toBe(1);
    expect(kcalFraction(-1, 2200)).toBe(0);
  });
  it('is 0 for missing data or zero target', () => {
    expect(kcalFraction(null, 2200)).toBe(0);
    expect(kcalFraction(100, null)).toBe(0);
    expect(kcalFraction(100, 0)).toBe(0);
    expect(kcalFraction(Number.NaN, 100)).toBe(0);
  });
});

describe('colors', () => {
  it('status colors', () => {
    expect(statusColor('well_rested', p)).toBe(p.good);
    expect(statusColor('okay', p)).toBe(p.warn);
    expect(statusColor('tired', p)).toBe(p.warn);
    expect(statusColor('no_data', p)).toBe(p.muted);
    expect(statusColor(null, p)).toBe(p.muted);
  });
  it('diet colors', () => {
    expect(dietColor('On track', p)).toBe(p.good);
    expect(dietColor('Under-eating', p)).toBe(p.warn);
    expect(dietColor('Overeating', p)).toBe(p.warn);
    expect(dietColor(null, p)).toBe(p.muted);
  });
});

describe('trend', () => {
  it('arrows', () => {
    expect(trendArrow('up')).toBe('↗');
    expect(trendArrow('flat')).toBe('→');
    expect(trendArrow('down')).toBe('↘');
    expect(trendArrow(null)).toBe('');
  });
  it('words', () => {
    expect(trendWord('up')).toBe('trending up');
    expect(trendWord('flat')).toBe('steady');
    expect(trendWord('down')).toBe('trending down');
    expect(trendWord(undefined)).toBe('');
  });
});

describe('formatting', () => {
  it('formatHours', () => {
    expect(formatHours(460)).toBe('7 h 40 min');
    expect(formatHours(480)).toBe('8 h');
    expect(formatHours(45)).toBe('45 min');
    expect(formatHours(0)).toBe('0 min');
    expect(formatHours(-10)).toBe('0 min');
    expect(formatHours(null)).toBe('0 min');
    expect(formatHours(Number.NaN)).toBe('0 min');
  });
  it('formatNumber', () => {
    expect(formatNumber(1420)).toBe('1,420');
    expect(formatNumber(999)).toBe('999');
    expect(formatNumber(1234567)).toBe('1,234,567');
    expect(formatNumber(null)).toBe('0');
  });
  it('formatKg', () => {
    expect(formatKg(78.4)).toBe('78.4 kg');
    expect(formatKg(78)).toBe('78 kg');
    expect(formatKg(78.449)).toBe('78.4 kg');
  });
});
