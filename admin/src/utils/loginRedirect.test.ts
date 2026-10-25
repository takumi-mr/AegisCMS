import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { SSO_ERROR_CODES, isSsoErrorCode, isSsoSession, takeLoginRedirect } from './loginRedirect';
import { getSessionToken, setSessionToken } from '../api/session';

describe('loginRedirect utility', () => {
  describe('SSO_ERROR_CODES and isSsoErrorCode', () => {
    it('recognizes all standard sso error codes', () => {
      for (const code of SSO_ERROR_CODES) {
        expect(isSsoErrorCode(code)).toBe(true);
      }
    });

    it('returns false for unrecognized codes', () => {
      expect(isSsoErrorCode('unknown_error')).toBe(false);
      expect(isSsoErrorCode('')).toBe(false);
    });
  });

  describe('isSsoSession', () => {
    it('returns true when session provider is oidc or saml', () => {
      expect(isSsoSession({ metadata: { provider: 'oidc' } })).toBe(true);
      expect(isSsoSession({ metadata: { provider: 'saml' } })).toBe(true);
    });

    it('returns false for password or missing provider', () => {
      expect(isSsoSession({ metadata: { provider: 'password' } })).toBe(false);
      expect(isSsoSession({ metadata: {} })).toBe(false);
      expect(isSsoSession({})).toBe(false);
    });
  });

  describe('takeLoginRedirect', () => {
    const originalWindow = global.window;

    beforeEach(() => {
      setSessionToken('');
      const mockReplaceState = vi.fn();
      global.window = {
        location: {
          hash: '',
          pathname: '/admin/',
          search: '',
        },
        history: {
          replaceState: mockReplaceState,
        },
      } as unknown as Window & typeof globalThis;
    });

    afterEach(() => {
      global.window = originalWindow;
    });

    it('returns null if neither session nor login_error is in the hash', () => {
      global.window.location.hash = '';
      expect(takeLoginRedirect()).toBeNull();
      expect(global.window.history.replaceState).not.toHaveBeenCalled();

      global.window.location.hash = '#unrelated=param';
      expect(takeLoginRedirect()).toBeNull();
      expect(global.window.history.replaceState).not.toHaveBeenCalled();
    });

    it('extracts session token, stores it, cleans url, and returns null error', () => {
      global.window.location.hash = '#session=ses_test_12345';
      const error = takeLoginRedirect();

      expect(error).toBeNull();
      expect(getSessionToken()).toBe('ses_test_12345');
      expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
    });

    it('extracts login_error, cleans url, and returns error code', () => {
      global.window.location.hash = '#login_error=idp_error';
      const error = takeLoginRedirect();

      expect(error).toBe('idp_error');
      expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
    });

    it('handles both session and login_error if present', () => {
      global.window.location.hash = '#session=ses_test_999&login_error=account_disabled';
      const error = takeLoginRedirect();

      expect(error).toBe('account_disabled');
      expect(getSessionToken()).toBe('ses_test_999');
      expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
    });
  });
});
