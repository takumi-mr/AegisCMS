import type { SessionInfo } from '../types';

// Mirrors the server's role levels (admin > editor > reviewer > contributor > viewer); see docs/authentication.md
const ROLE_LEVELS: Record<string, number> = {
  viewer: 1,
  contributor: 2,
  reviewer: 3,
  editor: 4,
  admin: 5,
};

const roleLevel = (session: SessionInfo | null): number =>
  session ? (ROLE_LEVELS[session.role] ?? 0) : 0;

/** Can write their own drafts, request a review and upload files (contributor and up) */
export const canWrite = (session: SessionInfo | null): boolean => roleLevel(session) >= 2;

/** Can review, proofread, approve and return content submitted for review (reviewer and up) */
export const canReview = (session: SessionInfo | null): boolean => roleLevel(session) >= 3;

/** Can publish, change anyone's content, manage categories, the shop, comments… (editor and up) */
export const canEdit = (session: SessionInfo | null): boolean => roleLevel(session) >= 4;

/** Can read audit logs */
export const isAdmin = (session: SessionInfo | null): boolean => roleLevel(session) >= 5;

/** Statuses a contributor may save (an editor publishes the content) */
export const CONTRIBUTOR_STATUSES = ['draft', 'pending'] as const;

/** Statuses a reviewer may save for an item under review */
export const REVIEWER_STATUSES = ['published', 'scheduled', 'draft', 'pending'] as const;

/**
 * Whether the session may change a stored item: editors change anything, reviewers change
 * any content in 'pending' status, contributors only their own drafts, items waiting for
 * review and published items (a change to a published item is saved as `pending`; the live
 * version stays until an editor/reviewer approves). Reviewers cannot create new content.
 * The server checks the same rules.
 */
export const canChangeContent = (
  session: SessionInfo | null,
  item: { author_id?: string | null; status: string } | null
): boolean => {
  if (canEdit(session)) return true;
  if (!canWrite(session) || !session) return false;
  if (!item) {
    // Reviewers cannot create new content
    return session.role !== 'reviewer';
  }
  if (session.role === 'reviewer') {
    return item.status === 'pending';
  }
  return (
    item.author_id === session.user_id &&
    (item.status === 'published' ||
      (CONTRIBUTOR_STATUSES as readonly string[]).includes(item.status))
  );
};

export const displayNameOf = (session: SessionInfo): string =>
  session.metadata?.display_name || session.user_id;
