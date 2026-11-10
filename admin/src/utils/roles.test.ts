import { describe, expect, it } from 'vitest';

import type { SessionInfo } from '../types';
import { canChangeContent, canEdit, canReview, canWrite, displayNameOf, isAdmin } from './roles';

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

    it('returns false for reviewer, contributor and viewer roles', () => {
      expect(canEdit(makeSession('reviewer'))).toBe(false);
      expect(canEdit(makeSession('contributor'))).toBe(false);
      expect(canEdit(makeSession('viewer'))).toBe(false);
    });

    it('returns false when session is null', () => {
      expect(canEdit(null)).toBe(false);
    });

    it('returns false for unknown role', () => {
      expect(canEdit(makeSession('guest'))).toBe(false);
    });
  });

  describe('canReview', () => {
    it('returns true for reviewer and up', () => {
      expect(canReview(makeSession('reviewer'))).toBe(true);
      expect(canReview(makeSession('editor'))).toBe(true);
      expect(canReview(makeSession('admin'))).toBe(true);
    });

    it('returns false for contributor, viewer, unknown roles and no session', () => {
      expect(canReview(makeSession('contributor'))).toBe(false);
      expect(canReview(makeSession('viewer'))).toBe(false);
      expect(canReview(makeSession('guest'))).toBe(false);
      expect(canReview(null)).toBe(false);
    });
  });

  describe('canWrite', () => {
    it('returns true for contributor and up', () => {
      expect(canWrite(makeSession('contributor'))).toBe(true);
      expect(canWrite(makeSession('reviewer'))).toBe(true);
      expect(canWrite(makeSession('editor'))).toBe(true);
      expect(canWrite(makeSession('admin'))).toBe(true);
    });

    it('returns false for viewer, unknown roles and no session', () => {
      expect(canWrite(makeSession('viewer'))).toBe(false);
      expect(canWrite(makeSession('guest'))).toBe(false);
      expect(canWrite(null)).toBe(false);
    });
  });

  describe('canChangeContent', () => {
    const own = (status: string) => ({ author_id: 'user_john', status });

    it('lets editors change any content', () => {
      expect(
        canChangeContent(makeSession('editor'), { author_id: 'other', status: 'published' })
      ).toBe(true);
    });

    it('lets contributors change only their own drafts, pending and published items', () => {
      const contributor = makeSession('contributor');
      expect(canChangeContent(contributor, null)).toBe(true);
      expect(canChangeContent(contributor, own('draft'))).toBe(true);
      expect(canChangeContent(contributor, own('pending'))).toBe(true);
      expect(canChangeContent(contributor, own('published'))).toBe(true);
      expect(canChangeContent(contributor, { author_id: 'other', status: 'published' })).toBe(
        false
      );
      expect(canChangeContent(contributor, own('scheduled'))).toBe(false);
      expect(canChangeContent(contributor, { author_id: 'other', status: 'draft' })).toBe(false);
    });

    it('lets reviewers change only pending content and forbids creating new content', () => {
      const reviewer = makeSession('reviewer');
      expect(canChangeContent(reviewer, null)).toBe(false);
      expect(canChangeContent(reviewer, { author_id: 'other', status: 'pending' })).toBe(true);
      expect(canChangeContent(reviewer, { author_id: 'other', status: 'draft' })).toBe(false);
      expect(canChangeContent(reviewer, { author_id: 'other', status: 'published' })).toBe(false);
      expect(canChangeContent(reviewer, { author_id: 'other', status: 'scheduled' })).toBe(false);
    });

    it('lets viewers change nothing', () => {
      expect(canChangeContent(makeSession('viewer'), null)).toBe(false);
    });
  });

  describe('isAdmin', () => {
    it('returns true for admin role', () => {
      expect(isAdmin(makeSession('admin'))).toBe(true);
    });

    it('returns false for editor, reviewer, contributor and viewer roles', () => {
      expect(isAdmin(makeSession('editor'))).toBe(false);
      expect(isAdmin(makeSession('reviewer'))).toBe(false);
      expect(isAdmin(makeSession('contributor'))).toBe(false);
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
