export * from './schemas';
import type { BlockNode } from './schemas';

export interface FieldDefinition {
  name: string;
  type: string;
  required: boolean;
  nullable: boolean;
  unique: boolean;
  index: boolean;
  max_length?: number;
  target?: string;
  indexed_paths: string[];
}

export interface SchemaDefinition {
  content_type: string;
  label: string;
  description?: string;
  fields: FieldDefinition[];
}

/** Related item resolved by the server for a `relation` field (`relations` in API responses) */
export interface ResolvedRelation {
  id: string;
  content_type: string;
  title: string;
  slug: string;
}

/**
 * Content item as returned by the content API.
 * Schema-specific custom fields are returned under `fields` and resolved relations under `relations`.
 */
export type ContentItem = import('./schemas').BaseContentItem & {
  fields?: Record<string, unknown>;
  relations?: Record<string, ResolvedRelation>;
};
export type CreateContentPayload = import('./schemas').CreateArticlePayload;

export type AuditAction =
  | 'content.create'
  | 'session.create'
  | 'session.delete'
  | 'storage.upload'
  | 'storage.delete'
  | 'plugin.error'
  | 'audit.purge'
  | 'access.denied'
  | 'theme.switch';

export type AuditOutcome = 'success' | 'failure' | 'denied';

export interface AuditEvent {
  id: string;
  occurred_at: string;
  action: AuditAction;
  outcome: AuditOutcome;
  actor: { user_id: string; role: string } | null;
  resource_type: string | null;
  resource_id: string | null;
  ip: string | null;
  user_agent: string | null;
  details: Record<string, unknown>;
}

export interface AuditLogQuery {
  action?: string;
  outcome?: string;
  actor_id?: string;
  resource_type?: string;
  resource_id?: string;
  since?: string;
  until?: string;
  limit?: number;
  offset?: number;
}

export interface AuditLogPage {
  items: AuditEvent[];
  limit: number;
  offset: number;
  /** Retention in days, or null when audit events are kept forever */
  retention_days: number | null;
}

/** Stored asset returned by `POST /api/v1/storage/upload` */
export interface StorageMeta {
  key: string;
  url: string;
  size: number;
  content_type: string;
}

/** Uploaded file listed by `GET /api/v1/storage/objects` */
export interface StorageObject extends StorageMeta {
  /** RFC 3339 timestamp, or null when the storage backend does not report one */
  last_modified: string | null;
}

export interface StorageObjectQuery {
  /** Key prefix, e.g. `2026/10/` */
  prefix?: string;
  /** Mime type prefix, e.g. `image/` */
  content_type?: string;
  /** Case-insensitive substring of the key */
  q?: string;
  limit?: number;
  offset?: number;
}

export interface StorageObjectPage {
  /** Newest first */
  objects: StorageObject[];
  /** Number of matching objects across all pages */
  total: number;
  limit: number;
  offset: number;
  /** True when the storage holds more files than the server scans for one listing */
  truncated: boolean;
}

/** A saved content item that refers to a stored file (`409` of `DELETE /api/v1/storage/objects/{key}`) */
export interface ObjectReference {
  content_type: string;
  id: string;
  slug: string;
  title: string;
}

export type UserRole = 'admin' | 'editor' | 'viewer';

/** Session returned by `POST /api/v1/auth/session` (login) and `GET /api/v1/auth/session` */
export interface SessionInfo {
  /** Bearer token (`ses_...`) */
  id: string;
  /** Username of the logged-in user */
  user_id: string;
  role: UserRole | string;
  created_at: string;
  expires_at: string;
  metadata: {
    provider?: string;
    display_name?: string;
    email?: string;
    [key: string]: unknown;
  };
}

/** Body of `POST /api/v1/preview` */
export interface PreviewRequest {
  /** Schema whose theme template renders the page (`article` → `article.html`) */
  content_type: string;
  title: string;
  slug: string;
  /** Custom fields, e.g. `category_id` */
  fields: Record<string, unknown>;
  blocks: BlockNode[];
}

/** Returned by `POST /api/v1/preview` */
export interface PreviewResult {
  /** Same body HTML the published page shows for these blocks */
  html: string;
  /** The whole themed page (null when the template failed; see `warnings`) */
  page_html: string | null;
  /** Blocks after the heading-id pipeline and plugin `transform-content-ast` hooks */
  blocks: BlockNode[];
  /** E.g. a plugin hook that failed (the preview then shows the built-in pipeline's result) */
  warnings: string[];
}

export type ThemeAuthor =
  | string
  | {
      name: string;
      email?: string;
      url?: string;
    };

export interface ThemeManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: ThemeAuthor;
  license?: string;
  preview_image?: string;
  tags: string[];
  homepage?: string;
  repository?: string;
  min_aegis_version?: string;
}

export interface ThemeInfo extends ThemeManifest {
  is_active: boolean;
  has_templates: boolean;
  has_assets: boolean;
  preview_url?: string;
}
