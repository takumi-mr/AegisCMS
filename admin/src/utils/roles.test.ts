import { describe, expect, it } from 'vitest';

import type { SessionInfo } from '../types';
import { canEdit, displayNameOf, isAdmin } from './roles';

const makeSession = (role: string, metadata: Record<string, string> = {}): SessionInfo => ({
  id: 'ses_test',
  user_id: 'user_john',
  role,
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata,
});

describe('roles utility', () => {
  describe('canEdit', () => {
    it('returns true for admin role', () => {
      expect(canEdit(makeSession('admin'))).toBe(true);
    });

    it('returns true for editor role', () => {
      expect(canEdit(makeSession('editor'))).toBe(true);
    });

    it('returns false for viewer role', () => {
      expect(canEdit(makeSession('viewer'))).toBe(false);
    });

    it('returns false when session is null', () => {
      expect(canEdit(null)).toBe(false);
    });

    it('returns false for unknown role', () => {
      expect(canEdit(makeSession('guest'))).toBe(false);
    });
  });

  describe('isAdmin', () => {
    it('returns true for admin role', () => {
      expect(isAdmin(makeSession('admin'))).toBe(true);
    });

    it('returns false for editor and viewer roles', () => {
      expect(isAdmin(makeSession('editor'))).toBe(false);
      expect(isAdmin(makeSession('viewer'))).toBe(false);
    });

    it('returns false when session is null', () => {
      expect(isAdmin(null)).toBe(false);
    });
  });

  describe('displayNameOf', () => {
    it('returns metadata display_name when present', () => {
      const session = makeSession('admin', { display_name: 'John Doe' });
      expect(displayNameOf(session)).toBe('John Doe');
    });

    it('falls back to user_id when display_name is missing', () => {
      const session = makeSession('admin', {});
      expect(displayNameOf(session)).toBe('user_john');
    });
  });
});
