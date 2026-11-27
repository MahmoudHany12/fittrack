import { initials, pickWeight, statusWord } from './helpers';

describe('home status helpers', () => {
  it('maps readiness and keys to words', () => {
    expect(statusWord('okay', 70)).toBe('Healthy');
    expect(statusWord('well_rested', 69)).toBe('Okay');
    expect(statusWord('well_rested', 39)).toBe('Tired');
    expect(statusWord('well_rested')).toBe('Healthy');
    expect(statusWord('tired', null)).toBe('Tired');
    expect(statusWord('no_data', null)).toBe('No data yet');
  });
  it('builds initials', () => {
    expect(initials('Alex Smith')).toBe('AS');
    expect(initials('alex')).toBe('A');
    expect(initials(null)).toBe('?');
  });
  it('prefers pending local weight', () => {
    expect(pickWeight(78, { kg: 80, pending: true })).toBe(80);
    expect(pickWeight(78, { kg: 80, pending: false })).toBe(78);
    expect(pickWeight(null, { kg: 80, pending: false })).toBe(80);
    expect(pickWeight(null, null)).toBeNull();
  });
});
