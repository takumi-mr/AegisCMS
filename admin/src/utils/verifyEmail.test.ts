import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  MIN_PASSWORD_LENGTH,
  REGISTRATION_ERROR_CODES,
  isRegistrationErrorCode,
  describeRegistrationError,
  takeVerifyToken,
  takeResetToken,
  takeInviteToken,
} from './verifyEmail';
import { ApiError } from '../api/client';
import type { TFunction } from 'i18next';

describe('verifyEmail utility', () => {
  it('defines MIN_PASSWORD_LENGTH and REGISTRATION_ERROR_CODES', () => {
    expect(MIN_PASSWORD_LENGTH).toBe(8);
    expect(REGISTRATION_ERROR_CODES).toContain('invalid_username');
    expect(REGISTRATION_ERROR_CODES).toContain('username_taken');
    expect(REGISTRATION_ERROR_CODES).toContain('password_too_short');
  });

  describe('isRegistrationErrorCode', () => {
    it('returns true for all known registration error codes', () => {
      for (const code of REGISTRATION_ERROR_CODES) {
        expect(isRegistrationErrorCode(code)).toBe(true);
      }
    });

    it('returns false for unknown codes or non-string values', () => {
      expect(isRegistrationErrorCode('some_random_code')).toBe(false);
      expect(isRegistrationErrorCode('')).toBe(false);
      expect(isRegistrationErrorCode(null)).toBe(false);
      expect(isRegistrationErrorCode(undefined)).toBe(false);
      expect(isRegistrationErrorCode(123)).toBe(false);
      expect(isRegistrationErrorCode({})).toBe(false);
    });
  });

  describe('describeRegistrationError', () => {
    const dummyT: TFunction = vi.fn((key: string, opts?: unknown) => {
      if (opts && typeof opts === 'object' && 'count' in opts) {
        return `${key}:${(opts as { count: number }).count}`;
      }
      return key;
    }) as unknown as TFunction;

    it('returns translated message for ApiError with recognized registration error code', () => {
      const err = new ApiError(400, 'Invalid email', undefined, 'invalid_email');
      const res = describeRegistrationError(err, dummyT);
      expect(res).toBe(`login.register.errors.invalid_email:${MIN_PASSWORD_LENGTH}`);
    });

    it('returns err.message for ApiError with unknown or missing code', () => {
      const errWithUnknownCode = new ApiError(400, 'Custom error msg', undefined, 'unknown_code');
      expect(describeRegistrationError(errWithUnknownCode, dummyT)).toBe('Custom error msg');

      const errWithoutCode = new ApiError(500, 'Internal error');
      expect(describeRegistrationError(errWithoutCode, dummyT)).toBe('Internal error');
    });

    it('returns common.unreachable fallback for non-ApiError instances', () => {
      expect(describeRegistrationError(new Error('Network error'), dummyT)).toBe(
        'common.unreachable'
      );
      expect(describeRegistrationError('some string', dummyT)).toBe('common.unreachable');
      expect(describeRegistrationError(null, dummyT)).toBe('common.unreachable');
    });
  });

  describe('hash token extractors', () => {
    const originalWindow = global.window;

    beforeEach(() => {
      // Setup window mock
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

    describe('takeVerifyToken', () => {
      it('returns token and cleans URL when verify-email hash parameter exists', () => {
        global.window.location.hash = '#verify-email=test-verify-token-123';
        const token = takeVerifyToken();
        expect(token).toBe('test-verify-token-123');
        expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
      });

      it('returns null when verify-email parameter is absent', () => {
        global.window.location.hash = '#other=param';
        expect(takeVerifyToken()).toBeNull();
        expect(global.window.history.replaceState).not.toHaveBeenCalled();
      });

      it('returns null when verify-email parameter is empty string', () => {
        global.window.location.hash = '#verify-email=';
        expect(takeVerifyToken()).toBeNull();
        expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
      });
    });

    describe('takeResetToken', () => {
      it('returns token and cleans URL when reset-password hash parameter exists', () => {
        global.window.location.hash = '#reset-password=reset-token-abc';
        const token = takeResetToken();
        expect(token).toBe('reset-token-abc');
        expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
      });

      it('returns null when reset-password parameter is absent', () => {
        global.window.location.hash = '#other=param';
        expect(takeResetToken()).toBeNull();
        expect(global.window.history.replaceState).not.toHaveBeenCalled();
      });
    });

    describe('takeInviteToken', () => {
      it('returns token and cleans URL when invite hash parameter exists', () => {
        global.window.location.hash = '#invite=invite-token-xyz';
        const token = takeInviteToken();
        expect(token).toBe('invite-token-xyz');
        expect(global.window.history.replaceState).toHaveBeenCalledWith(null, '', '/admin/');
      });

      it('returns null when invite parameter is absent', () => {
        global.window.location.hash = '#other=param';
        expect(takeInviteToken()).toBeNull();
        expect(global.window.history.replaceState).not.toHaveBeenCalled();
      });
    });
  });
});
