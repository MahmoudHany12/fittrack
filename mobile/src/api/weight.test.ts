import { displayWeight, formatWeight, kgToLb, lbToKg, newId, parseWeightInput } from './weight';

describe('weight units', () => {
  it('converts between kg and lb', () => {
    expect(kgToLb(lbToKg(180))).toBeCloseTo(180, 6);
    expect(displayWeight(80, 'imperial')).toBe('176.4');
    expect(formatWeight(80.04, 'metric')).toBe('80 kg');
  });

  it('parses input in either unit and a decimal comma', () => {
    expect(parseWeightInput('80,5', 'metric')).toBe(80.5);
    expect(parseWeightInput('176.4', 'imperial')).toBe(80.01);
  });

  it('rejects junk and out-of-range values', () => {
    expect(parseWeightInput('', 'metric')).toBeNull();
    expect(parseWeightInput('abc', 'metric')).toBeNull();
    expect(parseWeightInput('19.9', 'metric')).toBeNull();
    expect(parseWeightInput('401', 'metric')).toBeNull();
    expect(parseWeightInput('-5', 'metric')).toBeNull();
  });

  it('generates v4-shaped ids', () => {
    expect(newId()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(newId()).not.toBe(newId());
  });
});
