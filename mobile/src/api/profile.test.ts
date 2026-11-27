import { validateCalorieTarget } from './profile';

describe('validateCalorieTarget', () => {
  it('accepts values in range', () => {
    expect(validateCalorieTarget('2200')).toBeNull();
    expect(validateCalorieTarget(' 800 ')).toBeNull();
    expect(validateCalorieTarget('10000')).toBeNull();
  });
  it('rejects out of range and non-numbers', () => {
    expect(validateCalorieTarget('799')).not.toBeNull();
    expect(validateCalorieTarget('10001')).not.toBeNull();
    expect(validateCalorieTarget('')).not.toBeNull();
    expect(validateCalorieTarget('22.5')).not.toBeNull();
    expect(validateCalorieTarget('abc')).not.toBeNull();
  });
});
