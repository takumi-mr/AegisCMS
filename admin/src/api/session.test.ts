import { describe, it, expect, beforeEach } from 'vitest';
import { getSessionToken, setSessionToken } from './session';

describe('session API helper', () => {
  beforeEach(() => {
    setSessionToken('');
  });

  it('stores and retrieves session token in memory', () => {
    expect(getSessionToken()).toBe('');
    setSessionToken('ses_token_abc');
    expect(getSessionToken()).toBe('ses_token_abc');
  });

  it('clears session token when empty string is passed', () => {
    setSessionToken('ses_initial_xyz');
    expect(getSessionToken()).toBe('ses_initial_xyz');

    setSessionToken('');
    expect(getSessionToken()).toBe('');
  });
});
