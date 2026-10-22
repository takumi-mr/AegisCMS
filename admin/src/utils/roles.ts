import type { SessionInfo } from '../types';

// Mirrors the server's role levels (admin > editor > viewer); see docs/authentication.md
const ROLE_LEVELS: Record<string, number> = { viewer: 1, editor: 2, admin: 3 };

const roleLevel = (session: SessionInfo | null): number =>
  session ? (ROLE_LEVELS[session.role] ?? 0) : 0;

/** Can create content, categories and upload files */
export const canEdit = (session: SessionInfo | null): boolean => roleLevel(session) >= 2;

/** Can read audit logs */
export const isAdmin = (session: SessionInfo | null): boolean => roleLevel(session) >= 3;

export const displayNameOf = (session: SessionInfo): string =>
  session.metadata?.display_name || session.user_id;
