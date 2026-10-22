export * from './schemas';

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
  | 'plugin.error'
  | 'audit.purge';

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
