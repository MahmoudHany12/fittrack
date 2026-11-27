import {
  validateEmail,
  validateLogin,
  validatePassword,
  validateRegister,
} from './validation';

describe('validateEmail', () => {
  it('accepts a normal address', () => {
    expect(validateEmail('a@b.co')).toBeNull();
    expect(validateEmail('  a@b.co  ')).toBeNull();
  });
  it('rejects empty and malformed addresses', () => {
    expect(validateEmail('')).not.toBeNull();
    expect(validateEmail('nope')).not.toBeNull();
    expect(validateEmail('a@b')).not.toBeNull();
    expect(validateEmail('a b@c.com')).not.toBeNull();
  });
});

describe('validatePassword', () => {
  it('requires at least 10 characters', () => {
    expect(validatePassword('123456789')).not.toBeNull();
    expect(validatePassword('1234567890')).toBeNull();
  });
  it('rejects empty', () => {
    expect(validatePassword('')).not.toBeNull();
  });
});

describe('form validation', () => {
  it('register reports both fields', () => {
    expect(Object.keys(validateRegister('bad', 'short')).sort()).toEqual(['email', 'password']);
    expect(validateRegister('a@b.co', 'long-enough-pw')).toEqual({});
  });
  it('login does not enforce password length', () => {
    expect(validateLogin('a@b.co', 'x')).toEqual({});
    expect(validateLogin('a@b.co', '').password).toBeDefined();
  });
});
