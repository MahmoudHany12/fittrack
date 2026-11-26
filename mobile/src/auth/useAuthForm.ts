import { useState } from 'react';

import { ApiError } from '@/api/client';
import type { FieldErrors } from '@/validation';

/** Shared submit logic for the login and register forms. */
export function useAuthForm(
  validate: (email: string, password: string) => FieldErrors,
  submit: (email: string, password: string) => Promise<void>,
) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [errors, setErrors] = useState<FieldErrors>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function onSubmit() {
    const local = validate(email, password);
    setErrors(local);
    setFormError(null);
    if (Object.keys(local).length > 0) return;
    setBusy(true);
    try {
      await submit(email.trim(), password);
    } catch (e) {
      if (e instanceof ApiError) {
        const f = e.fields ?? {};
        const fieldErrors: FieldErrors = {};
        if (f.email) fieldErrors.email = f.email;
        if (f.password) fieldErrors.password = f.password;
        setErrors(fieldErrors);
        setFormError(Object.keys(fieldErrors).length > 0 ? null : e.message);
      } else {
        setFormError('Something went wrong. Please try again.');
      }
    } finally {
      setBusy(false);
    }
  }

  return { email, setEmail, password, setPassword, errors, formError, busy, onSubmit };
}
