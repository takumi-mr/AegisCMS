import type { SessionInfo } from '../types';

// Mirrors the server's role levels (admin > editor > contributor > viewer); see docs/authentication.md
const ROLE_LEVELS: Record<string, number> = { viewer: 1, contributor: 2, editor: 3, admin: 4 };

const roleLevel = (session: SessionInfo | null): number =>
  session ? (ROLE_LEVELS[session.role] ?? 0) : 0;

/** Can write their own drafts, request a review and upload files (contributor and up) */
export const canWrite = (session: SessionInfo | null): boolean => roleLevel(session) >= 2;

/** Can publish, change anyone's content, manage categories, the shop, comments… (editor and up) */
export const canEdit = (session: SessionInfo | null): boolean => roleLevel(session) >= 3;

/** Can read audit logs */
export const isAdmin = (session: SessionInfo | null): boolean => roleLevel(session) >= 4;

/** Statuses a contributor may save (an editor publishes the content) */
export const CONTRIBUTOR_STATUSES = ['draft', 'pending'] as const;

/**
 * Whether the session may change a stored item: editors change anything, contributors only
 * their own drafts, items waiting for review and published items (a change to a published item is
 * saved as `pending`; the live version stays until an editor approves). The server checks the same
 */
export const canChangeContent = (
  session: SessionInfo | null,
  item: { author_id?: string | null; status: string } | null
): boolean => {
  if (canEdit(session)) return true;
  if (!canWrite(session) || !session) return false;
  if (!item) return true;
  return (
    item.author_id === session.user_id &&
    (item.status === 'published' ||
      (CONTRIBUTOR_STATUSES as readonly string[]).includes(item.status))
  );
};

export const displayNameOf = (session: SessionInfo): string =>
  session.metadata?.display_name || session.user_id;
