import { setSessionToken } from '../api/session';

/** Messages for the `login_error` reasons of `GET /api/v1/auth/oidc/callback` */
const SSO_ERRORS: Record<string, string> = {
  idp_error: 'The identity provider did not complete the sign-in.',
  invalid_state: 'The sign-in link expired or was already used. Try again.',
  invalid_token: "The identity provider's response could not be verified.",
  missing_username: 'The identity provider did not send a user name or email address.',
  username_taken:
    'An AegisCMS account with your user name already exists but is not linked to this sign-in. Ask an administrator to link it.',
  not_provisioned: 'Your account has no access to AegisCMS. Ask an administrator to add you.',
  account_disabled: 'Your AegisCMS account is disabled.',
  server_error: 'Sign-in failed on the server. Check the server log.',
};

/**
 * Handles the redirect back from single sign-on: the server appends `#session=<token>` or
 * `#login_error=<reason>` to the Admin Studio URL. Stores the token, removes the fragment from
 * the address bar (and history) and returns the error message to show, if any.
 */
export const takeLoginRedirect = (): string | null => {
  const params = new URLSearchParams(window.location.hash.replace(/^#/, ''));
  const session = params.get('session');
  const error = params.get('login_error');
  if (session === null && error === null) return null;

  window.history.replaceState(null, '', window.location.pathname + window.location.search);
  if (session) setSessionToken(session);
  // Unknown reasons are messages from an on-user-authenticated plugin
  return error === null ? null : (SSO_ERRORS[error] ?? error);
};
