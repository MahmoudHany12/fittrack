import { buildBodyInput, parseNumber, validateBodyField, validateBodyForm, validateScanDate, type BodyForm } from './body';
import { ageOn, validateBirthDate, validateHeight } from './profile';
import { bmrSourceText, missingHint, tdeeSourceText } from './targets';

const NOW = new Date('2026-06-15T10:00:00.000Z');
const base: BodyForm = {
  date: '2026-06-15',
  weight_kg: '80,5',
  body_fat_pct: '',
  skeletal_muscle_kg: '',
  fat_mass_kg: '',
  visceral_fat_level: '',
  bmr_kcal: '',
  body_water_pct: '',
};

describe('body validation', () => {
  it('parses numbers with a decimal comma', () => {
    expect(parseNumber('80,5')).toBe(80.5);
    expect(parseNumber('abc')).toBeNull();
  });
  it('requires weight and enforces ranges', () => {
    expect(validateBodyField('weight_kg', '', true)).not.toBeNull();
    expect(validateBodyField('weight_kg', '19')).not.toBeNull();
    expect(validateBodyField('weight_kg', '401')).not.toBeNull();
    expect(validateBodyField('weight_kg', '80')).toBeNull();
    expect(validateBodyField('body_fat_pct', '')).toBeNull();
    expect(validateBodyField('body_fat_pct', '1')).not.toBeNull();
    expect(validateBodyField('body_fat_pct', '76')).not.toBeNull();
    expect(validateBodyField('visceral_fat_level', '7.5')).not.toBeNull();
    expect(validateBodyField('visceral_fat_level', '7')).toBeNull();
    expect(validateBodyField('bmr_kcal', '599')).not.toBeNull();
    expect(validateBodyField('body_water_pct', '55')).toBeNull();
  });
  it('validates the scan date', () => {
    expect(validateScanDate('2026-06-15', NOW)).toBeNull();
    expect(validateScanDate('2026-02-30', NOW)).not.toBeNull();
    expect(validateScanDate('15/06/2026', NOW)).not.toBeNull();
    expect(validateScanDate('2027-01-01', NOW)).not.toBeNull();
  });
  it('validates a whole form', () => {
    expect(validateBodyForm(base, NOW)).toEqual({});
    const bad = validateBodyForm({ ...base, weight_kg: '', bmr_kcal: '10' }, NOW);
    expect(Object.keys(bad).sort()).toEqual(['bmr_kcal', 'weight_kg']);
  });
});

describe('buildBodyInput', () => {
  it('builds a manual entry with nulls', () => {
    const b = buildBodyInput(base, 'id1', NOW);
    expect(b).toMatchObject({ id: 'id1', weight_kg: 80.5, body_fat_pct: null, source: 'manual' });
    expect(b.measured_at).toBe(NOW.toISOString());
  });
  it('marks scans as inbody and uses noon for past dates', () => {
    const b = buildBodyInput({ ...base, date: '2026-06-10', body_fat_pct: '18.2', bmr_kcal: '1750' }, 'id2', NOW);
    expect(b).toMatchObject({ body_fat_pct: 18.2, bmr_kcal: 1750, source: 'inbody' });
    expect(b.measured_at).toBe('2026-06-10T12:00:00.000Z');
  });
});

describe('profile body fields', () => {
  it('validates height', () => {
    expect(validateHeight('')).toBeNull();
    expect(validateHeight('178')).toBeNull();
    expect(validateHeight('99')).not.toBeNull();
    expect(validateHeight('251')).not.toBeNull();
    expect(validateHeight('tall')).not.toBeNull();
  });
  it('validates birth date and age range', () => {
    expect(validateBirthDate('', NOW)).toBeNull();
    expect(validateBirthDate('1990-05-01', NOW)).toBeNull();
    expect(validateBirthDate('1990-13-01', NOW)).not.toBeNull();
    expect(validateBirthDate('2020-01-01', NOW)).not.toBeNull();
    expect(validateBirthDate('1900-01-01', NOW)).not.toBeNull();
    expect(validateBirthDate('1990/05/01', NOW)).not.toBeNull();
  });
  it('computes age with birthdays', () => {
    expect(ageOn('2000-06-15', NOW)).toBe(26);
    expect(ageOn('2000-06-16', NOW)).toBe(25);
  });
});

describe('targets text', () => {
  it('explains sources in plain words', () => {
    expect(bmrSourceText('inbody')).toBe('Measured by your InBody scan');
    expect(bmrSourceText('katch_mcardle')).toBe('Estimated from your lean mass');
    expect(bmrSourceText('mifflin')).toBe('Estimated from height, age and sex');
    expect(tdeeSourceText('formula')).toMatch(/adapting/);
  });
  it('maps missing keys to a destination', () => {
    expect(missingHint('height_cm').to).toBe('/(tabs)/settings');
    expect(missingHint('body_fat_pct').to).toBe('/add/body');
    expect(missingHint('something_new').text).toBe('Add something new');
  });
});
