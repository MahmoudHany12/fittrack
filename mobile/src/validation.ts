export const MIN_PASSWORD_LENGTH = 10;

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function validateEmail(email: string): string | null {
  const v = email.trim();
  if (!v) return 'Enter your email address.';
  if (!EMAIL_RE.test(v)) return 'That does not look like an email address.';
  return null;
}

export function validatePassword(password: string): string | null {
  if (!password) return 'Enter a password.';
  if (password.length < MIN_PASSWORD_LENGTH) {
    return `Use at least ${MIN_PASSWORD_LENGTH} characters.`;
  }
  return null;
}

export type FieldErrors = { email?: string; password?: string };

export function validateRegister(email: string, password: string): FieldErrors {
  const errors: FieldErrors = {};
  const e = validateEmail(email);
  const p = validatePassword(password);
  if (e) errors.email = e;
  if (p) errors.password = p;
  return errors;
}

/** Login only checks that something was entered; the server decides the rest. */
export function validateLogin(email: string, password: string): FieldErrors {
  const errors: FieldErrors = {};
  const e = validateEmail(email);
  if (e) errors.email = e;
  if (!password) errors.password = 'Enter your password.';
  return errors;
}
