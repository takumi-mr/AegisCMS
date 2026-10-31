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
  comments_open?: boolean;
  fields?: Record<string, unknown>;
  relations?: Record<string, ResolvedRelation>;
};
/**
 * A content item in a list (`GET /contents/{type}?view=summary`): no blocks and no rendered HTML,
 * with a short `excerpt`. A full `ContentItem` is also a valid summary.
 */
export type ContentSummary = Omit<ContentItem, 'blocks' | 'rendered_html' | 'comments_open'> & {
  excerpt?: string;
};
/** A managed tag (`/api/v1/tags`); `usage` (the number of contents it is on) is in lists only */
export interface Tag {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  usage?: number;
  created_at: string;
  updated_at: string;
}
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
  | 'headers.update'
  | 'redirect.create'
  | 'redirect.update'
  | 'redirect.delete'
  | 'newsletter.send'
  | 'newsletter.delete'
  | 'notice.create'
  | 'notice.update'
  | 'notice.delete'
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
  | 'forum.category.create'
  | 'forum.category.update'
  | 'forum.category.delete'
  | 'forum.thread.create'
  | 'forum.thread.pin'
  | 'forum.thread.lock'
  | 'forum.thread.delete'
  | 'forum.post.create'
  | 'forum.post.delete'
  | 'forum.moderate'
  | 'forum.settings_update'
  | 'forum.report.create'
  | 'forum.report.resolve'
  | 'contact.update'
  | 'contact.delete'
  | 'notification.template_update'
  | 'notification.queue_update'
  | 'tag.create'
  | 'tag.update'
  | 'tag.delete'
  | 'content.tags';

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
  /** True when two-factor authentication (TOTP) is enabled */
  totp_enabled?: boolean;
}

export interface TotpSetupResult {
  secret: string;
  otpauth_url: string;
  qr_code_base64: string;
}

export interface Enable2faResult {
  enabled: boolean;
  backup_codes: string[];
}

export interface LoginMfaChallenge {
  mfa_required: true;
  challenge_token: string;
  user_id: string;
}

export type LoginResponse = SessionInfo | LoginMfaChallenge;

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

/** Text in a plugin manifest: one string, or one per language (`{ "en": ..., "ja": ... }`) */
export type LocalizedText = string | Record<string, string>;

export type PluginBlockFieldType =
  'string' | 'text' | 'url' | 'number' | 'boolean' | 'select' | 'image';

/** An input of a plugin's custom block; its value goes to `attributes[name]` */
export interface PluginBlockField {
  name: string;
  type: PluginBlockFieldType;
  label: LocalizedText;
  required?: boolean;
  default?: unknown;
  options?: { value: string; label: LocalizedText }[];
  placeholder?: LocalizedText;
  help?: LocalizedText;
}

/** A custom block an enabled plugin provides (`GET /api/v1/blocks`) */
export interface PluginBlockDefinition {
  /** Block name stored in the body: `<plugin>/<block>` */
  name: string;
  plugin: string;
  plugin_display_name: string;
  label: LocalizedText;
  description?: LocalizedText;
  fields: PluginBlockField[];
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

/** Dynamic system and runtime settings (`GET` / `PUT /api/v1/system/settings`) */
export interface SystemSettingsValues {
  maintenance_mode: boolean;
  maintenance_message: string;
  max_upload_mb: number;
  cache_enabled: boolean;
  cache_ttl_secs: number;
  registration_enabled: boolean;
  max_login_attempts: number;
  comments_enabled: boolean;
  forum_enabled: boolean;
}

export interface SystemOverrides {
  maintenance_mode: boolean | null;
  maintenance_message: string | null;
  max_upload_mb: number | null;
  cache_enabled: boolean | null;
  cache_ttl_secs: number | null;
  registration_enabled: boolean | null;
  max_login_attempts: number | null;
  comments_enabled: boolean | null;
  forum_enabled: boolean | null;
}

export interface SystemSettingsResponse {
  settings: SystemSettingsValues;
  overrides: SystemOverrides;
  defaults: SystemSettingsValues;
}

export type UpdateSystemSettings = Partial<SystemOverrides>;

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

export interface CustomHeader {
  name: string;
  value: string;
}

export interface HeaderDefaults {
  content_security_policy: string;
  x_frame_options: string;
  x_content_type_options: string;
  referrer_policy: string;
  permissions_policy: string;
  cross_origin_opener_policy: string;
}

/** HTTP and browser defense headers (`GET` / `PUT /api/v1/headers`) */
export interface HeaderSettings {
  content_security_policy: string | null;
  x_frame_options: string | null;
  x_content_type_options: string | null;
  referrer_policy: string | null;
  permissions_policy: string | null;
  cross_origin_opener_policy: string | null;
  custom_headers: CustomHeader[];
  defaults: HeaderDefaults;
}

export interface UpdateHeaderSettings {
  content_security_policy?: string | null;
  x_frame_options?: string | null;
  x_content_type_options?: string | null;
  referrer_policy?: string | null;
  permissions_policy?: string | null;
  cross_origin_opener_policy?: string | null;
  custom_headers?: CustomHeader[];
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
  /** Hosts whose `<iframe>`s the plugin's blocks may embed (also added to the CSP `frame-src`) */
  embed?: { frame_domains: string[] } | null;
}

export interface PluginResourceLimits {
  max_memory_mb: number;
  max_fuel_units: number;
  execution_timeout_ms: number;
}

/** A name, or a name with contact details */
export type PackageAuthor = string | { name: string; email?: string; url?: string };

export type PluginSettingType =
  'string' | 'text' | 'url' | 'number' | 'boolean' | 'select' | 'color';

/** A setting a plugin declares in its manifest (`settings`); the value is stored as a string */
export interface PluginSettingDefinition {
  key: string;
  type: PluginSettingType;
  label: LocalizedText;
  description?: LocalizedText;
  /** What the plugin uses while the setting is empty (shown as the placeholder; never saved) */
  default?: unknown;
  options?: { value: string; label: LocalizedText }[];
  min?: number;
  max?: number;
  max_length?: number;
  placeholder?: LocalizedText;
}

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
  /** Settings Admin Studio shows as form fields (the others stay in the key/value list) */
  settings?: PluginSettingDefinition[];
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

/** A redirect rule (`/api/v1/redirects`): visitors of a missing page `from` are sent to `to` */
export interface Redirect {
  id: string;
  /** Old path, without the base path */
  from: string;
  /** A path of the site (the base path is added) or an http(s) URL */
  to: string;
  /** 301 (permanent) or 302 (temporary) */
  status: number;
  /** Kept automatically when a published slug changed */
  auto: boolean;
  created_at: string;
  updated_at: string;
}

export type NewsletterStatus = 'pending' | 'active' | 'unsubscribed';

/** A newsletter subscriber (`/api/v1/newsletter/subscribers`); the link tokens never leave the server */
export interface NewsletterSubscriber {
  id: string;
  email: string;
  status: NewsletterStatus;
  ip: string | null;
  created_at: string;
  confirmed_at: string | null;
  unsubscribed_at: string | null;
}

export interface NewsletterCounts {
  pending: number;
  active: number;
  unsubscribed: number;
}

/** One time published content was sent to the subscribers (`/api/v1/newsletter/issues`) */
export interface NewsletterIssue {
  id: string;
  content_type: string;
  content_id: string;
  title: string;
  url: string;
  recipients: number;
  created_at: string;
  created_by: string | null;
}

export type NoticeStyle = 'info' | 'success' | 'warning' | 'alert';

/** A site notice (`/api/v1/notices`): a banner on every public page during its period */
export interface Notice {
  id: string;
  message: string;
  /** A path of the site or an http(s) URL; empty for no link */
  link_url: string;
  /** Empty uses the theme's default wording */
  link_label: string;
  style: NoticeStyle;
  /** RFC 3339; null for "right away" / "no end" */
  start_at: string | null;
  end_at: string | null;
  dismissible: boolean;
  enabled: boolean;
  /** Where it is now: shown, waiting for its start, past its end, or switched off */
  status: 'active' | 'scheduled' | 'expired' | 'disabled';
  created_at: string;
  updated_at: string;
}

export interface NoticePayload {
  message: string;
  link_url: string;
  link_label: string;
  style: NoticeStyle;
  start_at: string | null;
  end_at: string | null;
  dismissible: boolean;
  enabled: boolean;
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
  /** Content types that accept comments (`[comments].content_types`) */
  content_types?: string[];
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
  status?: CommentStatus;
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
  status?: CommentStatus;
  is_deleted: boolean;
  ip: string | null;
  user_agent: string | null;
  created_at: string;
  updated_at: string;
}

export interface ForumReport {
  id: string;
  target_type: 'thread' | 'post';
  target_id: string;
  thread_id: string;
  reason: string;
  reporter_id: string | null;
  reporter_ip: string | null;
  status: 'open' | 'resolved' | 'dismissed';
  resolved_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface ForumModerationSettings {
  policy: CommentPolicy;
  overrides: { require_approval: boolean | null; auto_approve_known: boolean | null };
  defaults: CommentPolicy;
  config: {
    require_approval: boolean;
    auto_approve_known: boolean;
    max_links: number;
    hold_words: string[];
    spam_words: string[];
    max_per_ip_per_hour: number;
  };
}

/** A value's totals in `GET /api/v1/analytics/summary` (a page, a referrer host or a device type) */
export interface AnalyticsTotal {
  /** The page path, referrer host ('' = direct / no referrer) or device (`desktop` / `mobile` / `tablet`) */
  value: string;
  pageviews: number;
  visitors: number;
}

/** One day in `GET /api/v1/analytics/summary` */
export interface AnalyticsDay {
  /** `YYYY-MM-DD` (UTC) */
  day: string;
  pageviews: number;
  visitors: number;
}

/** Comparison with the previous period in `GET /api/v1/analytics/summary` */
export interface AnalyticsComparison {
  from: string;
  to: string;
  pageviews: number;
  visitors: number;
  pageviews_change_pct: number | null;
  visitors_change_pct: number | null;
}

/** `GET /api/v1/analytics/summary` (404 when [analytics] is disabled) */
export interface AnalyticsSummary {
  from: string;
  to: string;
  pageviews: number;
  /** The sum of each day's unique visitors (the same person on two days counts twice) */
  visitors: number;
  /** Every day of the range, oldest first (0 for days without visits) */
  daily: AnalyticsDay[];
  top_pages: AnalyticsTotal[];
  /** Entry page views from other sites; '' is direct visits */
  top_referrers: AnalyticsTotal[];
  devices: AnalyticsTotal[];
  top_utm_sources: AnalyticsTotal[];
  top_utm_campaigns: AnalyticsTotal[];
  top_utm_mediums: AnalyticsTotal[];
  top_search_queries: AnalyticsTotal[];
  top_search_empty: AnalyticsTotal[];
  previous?: AnalyticsComparison | null;
}

/** A field of a contact form (`[[contact.forms.fields]]` in the configuration file) */
export interface ContactFieldConfig {
  name: string;
  label: string;
  type: 'text' | 'email' | 'tel' | 'textarea' | 'select' | 'checkbox' | 'file';
  required: boolean;
  placeholder: string;
  options: string[];
  max_length: number | null;
}

/** `GET /api/v1/contact/forms` (404 when [contact] is disabled) */
export interface ContactForm {
  /** Name used by the block and the URL (`/contact/{name}`) */
  name: string;
  title: string;
  /** Public page path (with the base path) */
  path: string;
  fields: ContactFieldConfig[];
  auto_reply: boolean;
}

export type ContactStatus = 'new' | 'read' | 'done' | 'spam';

/** A file attached to a contact message (downloaded through the API; it has no public URL) */
export interface ContactAttachment {
  filename: string;
  size: number;
  content_type: string;
}

/** One answered field of a contact message (checkboxes are `yes` or empty; files carry the file name) */
export interface ContactFieldValue {
  name: string;
  label: string;
  value: string;
  file?: ContactAttachment;
}

/** A message in the contact inbox (`GET /api/v1/contact/messages`) */
export interface ContactMessage {
  id: string;
  form: string;
  /** The form's title (its name when the form is no longer configured) */
  form_title: string;
  name: string;
  email: string;
  subject: string;
  fields: ContactFieldValue[];
  status: ContactStatus;
  ip: string | null;
  user_agent: string | null;
  created_at: string;
  updated_at: string;
}

export interface ContactMessagePage {
  messages: ContactMessage[];
  total: number;
  /** Messages per status (all forms) */
  counts: Partial<Record<ContactStatus, number>>;
}
