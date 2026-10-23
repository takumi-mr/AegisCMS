import { setSessionToken } from '../api/session';

/** The `login_error` reasons of `GET /api/v1/auth/oidc/callback` (messages in `login.sso_errors`) */
export const SSO_ERROR_CODES = [
  'idp_error',
  'invalid_state',
  'invalid_token',
  'missing_username',
  'username_taken',
  'not_provisioned',
  'account_disabled',
  'server_error',
] as const;

export type SsoErrorCode = (typeof SSO_ERROR_CODES)[number];

export const isSsoErrorCode = (value: string): value is SsoErrorCode =>
  (SSO_ERROR_CODES as readonly string[]).includes(value);

/**
 * Handles the redirect back from single sign-on: the server appends `#session=<token>` or
 * `#login_error=<reason>` to the Admin Studio URL. Stores the token, removes the fragment from
 * the address bar (and history) and returns the error to show, if any: a reason code
 * (`isSsoErrorCode`) or a message from an on-user-authenticated plugin.
 */
export const takeLoginRedirect = (): string | null => {
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const session = params.get('session');
  const error = params.get('login_error');
  if (session === null && error === null) return null;

  window.history.replaceState(null, '', window.location.pathname + window.location.search);
  if (session) setSessionToken(session);
  return error;
};
