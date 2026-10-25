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
  | 'content.update'
  | 'content.delete'
  | 'content.publish'
  | 'content.unpublish'
  | 'session.create'
  | 'session.delete'
  | 'storage.upload'
  | 'storage.delete'
  | 'plugin.error'
  | 'audit.purge'
  | 'access.denied'
  | 'theme.switch'
  | 'site.update'
  | 'seo.update'
  | 'user.create'
  | 'user.update'
  | 'user.delete'
  | 'user.register'
  | 'order.create'
  | 'order.update'
  | 'notification.send'
  | 'plugin.settings_update'
  | 'plugin.enable'
  | 'plugin.disable'
  | 'plugin.reorder'
  | 'plugin.reset'
  | 'comment.create'
  | 'comment.moderate'
  | 'comment.update'
  | 'comment.delete'
  | 'comment.settings_update'
  | 'notification.template_update'
  | 'notification.queue_update';

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
  /** Include avatar images (`avatars/`); defaults to false unless prefix starts with "avatars" */
  include_avatars?: boolean;
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

/** User account returned by the user management API (`/api/v1/users`) */
export interface UserAccount {
  id: string;
  /** Login name (unique, cannot be changed); the session's `user_id` */
  username: string;
  email: string | null;
  display_name: string | null;
  role: UserRole | string;
  disabled: boolean;
  /** True when self-registered and waiting for admin approval */
  pending_approval?: boolean;
  /** Public URL of the avatar image, or null when none is set */
  avatar_url: string | null;
  created_at: string;
  updated_at: string;
}

/** Body of `POST /api/v1/users` */
export interface CreateUserPayload {
  username: string;
  password: string;
  role: UserRole;
  email?: string;
  display_name?: string;
}

/**
 * Body of `PATCH /api/v1/users/{username}`; omitted fields stay as they are and an empty
 * `email` / `display_name` clears it. `role` / `disabled` are for admins editing someone else,
 * and changing one's own password needs `current_password`.
 */
export interface UpdateUserPayload {
  email?: string;
  display_name?: string;
  role?: UserRole;
  disabled?: boolean;
  password?: string;
  current_password?: string;
}

/** A single sign-on button: its label and the server URL that starts the sign-in */
export interface SsoProvider {
  display_name: string;
  login_url: string;
}

/** Sign-in methods offered by the server (`GET /api/v1/auth/providers`) */
export interface AuthProviders {
  local: boolean;
  /** Single sign-on with OpenID Connect, or null when it is not enabled */
  oidc: SsoProvider | null;
  /** Single sign-on with SAML 2.0, or null when it is not enabled; missing on older servers */
  saml?: SsoProvider | null;
  /** Self-registration (`[auth.registration]`); missing on older servers */
  registration?: boolean;
  /** Password reset via email (`[auth.password_reset]`) */
  password_reset?: boolean;
  /** Invite links (`[auth.invitation]`) */
  invitation?: boolean;
}

/** Body of `POST /api/v1/auth/registration` */
export interface RegistrationPayload {
  username: string;
  email: string;
  password: string;
  display_name?: string;
}

/** Response of `POST /api/v1/auth/registration/verify` */
export interface RegistrationResult {
  /** `active`: can sign in now; `pending_approval`: disabled until an admin enables it */
  status: 'active' | 'pending_approval';
  username: string;
}

/** Invitation info returned by `GET /api/v1/auth/invitations/info?token=...` */
export interface InvitationInfo {
  valid: boolean;
  email: string;
  role: string;
  display_name: string | null;
}

/** Body of `POST /api/v1/auth/invitations` */
export interface CreateInvitationPayload {
  email: string;
  role: string;
  display_name?: string;
}

/** Body of `POST /api/v1/auth/invitations/accept` */
export interface AcceptInvitePayload {
  token: string;
  username: string;
  password: string;
  display_name?: string;
}

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

export type OrderStatus =
  'pending_payment' | 'paid' | 'shipped' | 'completed' | 'cancelled' | 'refunded';

export interface OrderRefundRecord {
  id: string;
  amount: number;
  currency: string;
  refund_ref: string;
  reason?: string | null;
  created_at: string;
}

export interface RefundOrderPayload {
  amount?: number;
  reason?: string;
  restock?: boolean;
}

/** One order line (product × quantity); prices come from the product, in the currency's minor unit */
export interface OrderLine {
  product_id: string;
  slug: string;
  title: string;
  sku: string | null;
  unit_price: number;
  quantity: number;
  line_total: number;
}

/** Shipping, tax or discount (negative) added by `calculate-order` plugins */
export interface OrderAdjustment {
  kind: string;
  label: string;
  amount: number;
  code: string | null;
  /** Plugin that added it */
  source: string;
}

export interface ShippingAddress {
  name: string;
  postal_code: string;
  region: string;
  city: string;
  line1: string;
  line2: string;
  phone: string;
  country: string;
}

/** Order as staff see it (`GET /api/v1/orders`) */
export interface Order {
  id: string;
  /** Number shown to the customer */
  number: string;
  status: OrderStatus;
  currency: string;
  lines: OrderLine[];
  adjustments: OrderAdjustment[];
  subtotal: number;
  total: number;
  customer_name: string;
  customer_email: string;
  shipping_address: ShippingAddress | null;
  coupon_code: string | null;
  payment_provider: string;
  payment_ref: string | null;
  transaction_id: string | null;
  refunded_amount: number;
  refunds: OrderRefundRecord[];
  payment_instructions: string | null;
  /** Deadline of an unpaid order's stock reservation */
  expires_at: string | null;
  metadata: Record<string, unknown>;
  note: string | null;
  created_at: string;
  updated_at: string;
  paid_at: string | null;
  /** Whether staff may confirm the payment by hand (payments confirmed by webhook may not) */
  can_mark_paid: boolean;
  can_refund: boolean;
  supports_online_refund: boolean;
}

export interface OrderPage {
  orders: Order[];
  total: number;
  limit: number;
  offset: number;
}

export interface SalesReportDay {
  day: string;
  currency: string;
  orders: number;
  gross: number;
  refunded: number;
}

export interface SalesReport {
  start: string;
  end: string;
  days: SalesReportDay[];
}

/** Name and favicon of the public site (`GET` / `PUT /api/v1/site`) */
export interface SiteSettings {
  /** Set in Admin Studio, or null to use the configuration file's value */
  title: string | null;
  favicon: string | null;
  /** Values of the configuration file (`[theme].site_title` / `favicon`) */
  defaults: {
    title: string;
    favicon: string | null;
  };
}

/** SEO and crawler settings (`GET` / `PUT /api/v1/seo`) */
export interface SeoSettings {
  robots_txt: string | null;
  meta_description: string | null;
  meta_keywords: string | null;
  canonical_base_url: string | null;
  og_image: string | null;
  noindex_all: boolean;
  twitter_card: 'summary' | 'summary_large_image' | null;
  defaults: {
    robots_txt: string;
    meta_description: string;
  };
}

/** A notification kind with built-in sample values (`GET /api/v1/notifications`) */
export interface NotificationKindInfo {
  kind: string;
  /** Has a template for a specific person (customer, author…) */
  person: boolean;
  /** Has a template for the staff (its own or the shared one) */
  staff: boolean;
  /** Listed in `[notifications].disabled_events` */
  disabled: boolean;
}

/** Notification setup and queue counts (`GET /api/v1/notifications`) */
export interface NotificationStatus {
  enabled: boolean;
  channels: string[];
  locale: string;
  html: boolean;
  /** The queue is kept in the database (`[notifications.queue].persistent`) */
  persistent: boolean;
  max_attempts: number;
  /** `[notifications.email].staff_recipients` is set */
  staff_recipients: boolean;
  queue: { pending: number; failed: number };
  kinds: NotificationKindInfo[];
}

export type NotificationQueueStatus = 'pending' | 'failed';

export interface NotificationQueueItem {
  id: string;
  kind: string;
  channel: string;
  /** Email address, or `staff` */
  recipient: string;
  subject: string;
  status: NotificationQueueStatus;
  attempts: number;
  last_error: string | null;
  next_attempt_at: string;
  /** A server is sending it right now */
  sending: boolean;
  created_at: string;
  updated_at: string;
}

export interface NotificationQueuePage {
  items: NotificationQueueItem[];
  total: number;
}

/** Where a notification template comes from */
export type NotificationTemplateSource = 'custom' | 'file' | 'builtin';

export interface NotificationTemplateInfo {
  name: string;
  source: NotificationTemplateSource;
}

export interface NotificationTemplateBody {
  source: NotificationTemplateSource;
  content: string;
}

export interface NotificationTemplateDetail {
  name: string;
  /** The template in use (null: none yet, a new one can be written) */
  current: NotificationTemplateBody | null;
  /** What applies after resetting the edit (file or built-in) */
  default: NotificationTemplateBody | null;
}

export interface NotificationPreview {
  subject: string;
  body: string;
  html: string | null;
}

export interface NotificationTestDelivery {
  channel: string;
  recipient: string;
  subject: string;
  ok: boolean;
  error: string | null;
}

export interface UpdateSeoSettingsPayload {
  robots_txt?: string | null;
  meta_description?: string | null;
  meta_keywords?: string | null;
  canonical_base_url?: string | null;
  og_image?: string | null;
  noindex_all?: boolean;
  twitter_card?: 'summary' | 'summary_large_image' | null;
}

/** Public shop settings (`GET /api/v1/shop/config`) */
export interface ShopConfig {
  enabled: boolean;
  /** ISO 4217 code in lower case; amounts are in its minor unit */
  currency: string;
  /** Content type of products (`product`) */
  product_type: string;
  /** `manual` / `stripe`, or null when the shop is disabled */
  provider: string | null;
}

export interface PluginCapabilities {
  network?: { allowed_domains: string[] } | null;
  storage?: { kv_namespace: string } | null;
  hooks: string[];
}

export interface PluginResourceLimits {
  max_memory_mb: number;
  max_fuel_units: number;
  execution_timeout_ms: number;
}

/** A name, or a name with contact details */
export type PackageAuthor = string | { name: string; email?: string; url?: string };

export interface PluginManifest {
  name: string;
  version: string;
  display_name?: string;
  description?: string;
  author?: PackageAuthor;
  license?: string;
  homepage?: string;
  capabilities: PluginCapabilities;
  resource_limits: PluginResourceLimits;
}

export interface PluginInfo {
  /** Install directory name (`plugins/<name>/`) */
  name: string;
  /** Null when the plugin is not installed or its manifest cannot be read */
  manifest: PluginManifest | null;
  /** In the enabled list (hooks run in that order) */
  is_enabled: boolean;
  is_installed: boolean;
  /** Why the plugin cannot be enabled (not installed, invalid manifest, ...) */
  problem?: string;
  settings?: Record<string, string>;
}

/** `GET /api/v1/plugins/order`: the enabled plugins in hook order */
export interface PluginOrder {
  enabled: string[];
  /** `[plugins].enabled` of the configuration file */
  configured: string[];
  /** The list was changed in Admin Studio (saved in the database, overrides `configured`) */
  customized: boolean;
}

export interface ScheduleItem {
  id: string;
  content_type: string;
  title: string;
  slug: string;
  status: import('./schemas').ContentStatus;
  published_at?: string | null;
  unpublish_at?: string | null;
  author_id?: string | null;
  updated_at: string;
}

export interface ScheduleResponse {
  items: ScheduleItem[];
  total: number;
}

export interface ProductVariant {
  id: string;
  product_id: string;
  sku: string | null;
  title: string;
  price_override: number | null;
  stock: number | null;
  attributes: Record<string, string>;
  position: number;
  created_at: string;
  updated_at: string;
}

export interface UpsertVariantPayload {
  id?: string;
  sku?: string | null;
  title: string;
  price_override?: number | null;
  stock?: number | null;
  attributes?: Record<string, string>;
  position?: number;
}

/** Moderation status of a reader's comment */
export type CommentStatus = 'pending' | 'approved' | 'spam' | 'trash';

/** A comment as Admin Studio sees it (`GET /api/v1/comments`) */
export interface AdminComment {
  id: string;
  content_type: string;
  content_id: string;
  /** The content the comment is on (`null` once that content is deleted) */
  content: { title: string; slug: string; url: string; comments_open: boolean } | null;
  parent_id: string | null;
  /** Name of the person this comment replies to */
  parent_author: string | null;
  author_name: string;
  author_email: string;
  /** Username when an Admin Studio user wrote it (a staff reply) */
  author_user_id: string | null;
  body: string;
  status: CommentStatus;
  ip: string | null;
  user_agent: string | null;
  notify_replies: boolean;
  created_at: string;
  updated_at: string;
}

export interface CommentPage {
  comments: AdminComment[];
  total: number;
  /** Comments per status, over all comments */
  counts: Record<CommentStatus, number>;
}

export interface CommentPolicy {
  /** Comments wait for approval before they are published */
  require_approval: boolean;
  /** While approval is on, people with an approved comment (same name and email) skip the wait */
  auto_approve_known: boolean;
}

/** `GET` / `PUT /api/v1/comments/settings` */
export interface CommentSettings {
  /** The values in effect */
  policy: CommentPolicy;
  /** Values changed from Admin Studio (`null`: the configuration file's value) */
  overrides: { require_approval: boolean | null; auto_approve_known: boolean | null };
  /** `[comments.moderation]` in the configuration file */
  defaults: CommentPolicy;
}

/** Bulk action on comments: a new status, or `delete` (permanently) */
export type CommentBulkAction = CommentStatus | 'delete';

// =========================================================================
// Forum (掲示板モデレーション)
// =========================================================================

export interface ForumCategory {
  id: string;
  slug: string;
  name: string;
  description: string;
  position: number;
  is_private: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateForumCategoryPayload {
  slug: string;
  name: string;
  description: string;
  position: number;
  is_private: boolean;
}

export interface UpdateForumCategoryPayload {
  slug: string;
  name: string;
  description: string;
  position: number;
  is_private: boolean;
}

export interface ForumThread {
  id: string;
  category_id: string;
  title: string;
  author_id: string | null;
  author_name: string;
  is_pinned: boolean;
  is_locked: boolean;
  post_count: number;
  last_post_at: string;
  created_at: string;
  updated_at: string;
}

export interface ForumPost {
  id: string;
  thread_id: string;
  post_number: number;
  author_id: string | null;
  author_name: string;
  body: string;
  reply_to_post_id: string | null;
  reply_to_number: number | null;
  is_deleted: boolean;
  ip: string | null;
  user_agent: string | null;
  created_at: string;
  updated_at: string;
}
