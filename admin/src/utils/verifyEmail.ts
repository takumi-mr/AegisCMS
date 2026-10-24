import type { TFunction } from 'i18next';
import { ApiError } from '../api/client';

/** Minimum password length (`MIN_PASSWORD_LEN` on the server) */
export const MIN_PASSWORD_LENGTH = 8;

/** The `code` values of the registration endpoints (messages in `login.register.errors`) */
export const REGISTRATION_ERROR_CODES = [
  'invalid_username',
  'username_taken',
  'invalid_email',
  'email_domain_not_allowed',
  'password_too_short',
  'email_taken',
  'invalid_token',
  'rate_limited',
  'server_error',
] as const;

export type RegistrationErrorCode = (typeof REGISTRATION_ERROR_CODES)[number];

export const isRegistrationErrorCode = (value: unknown): value is RegistrationErrorCode =>
  typeof value === 'string' && (REGISTRATION_ERROR_CODES as readonly string[]).includes(value);

/**
 * Takes the token of an email confirmation link (`#verify-email=<token>` on the Admin Studio
 * URL) and removes it from the address bar and history, so a reload does not reuse it
 */
export const takeVerifyToken = (): string | null => {
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const token = params.get('verify-email');
  if (token === null) return null;
  window.history.replaceState(null, '', window.location.pathname + window.location.search);
  return token || null;
};

/**
 * Takes the token of a password reset link (`#reset-password=<token>`)
 */
export const takeResetToken = (): string | null => {
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const token = params.get('reset-password');
  if (token === null) return null;
  window.history.replaceState(null, '', window.location.pathname + window.location.search);
  return token || null;
};

/**
 * Takes the token of an invitation link (`#invite=<token>`)
 */
export const takeInviteToken = (): string | null => {
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const token = params.get('invite');
  if (token === null) return null;
  window.history.replaceState(null, '', window.location.pathname + window.location.search);
  return token || null;
};

/** Message for a failed registration or confirmation (`login.register.errors.<code>`) */
export const describeRegistrationError = (err: unknown, t: TFunction): string => {
  if (err instanceof ApiError) {
    if (isRegistrationErrorCode(err.code)) {
      return t(`login.register.errors.${err.code}`, { count: MIN_PASSWORD_LENGTH });
    }
    return err.message;
  }
  // fetch rejects with a TypeError when the server cannot be reached
  return t('common.unreachable');
};
