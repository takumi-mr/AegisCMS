import i18n from '../i18n';
import type {
  AcceptInvitePayload,
  AnalyticsSummary,
  AuditLogPage,
  AuditLogQuery,
  AuthProviders,
  CommentBulkAction,
  CommentPage,
  CommentSettings,
  CommentStatus,
  ContactForm,
  ContactMessagePage,
  ContactStatus,
  ContentItem,
  ContentSummary,
  CreateCategoryPayload,
  CreateContentPayload,
  CreateForumCategoryPayload,
  CreateInvitationPayload,
  CreateUserPayload,
  ForumCategory,
  ForumModerationSettings,
  ForumPost,
  ForumReport,
  ForumThread,
  HeaderSettings,
  InvitationInfo,
  NotificationPreview,
  NotificationQueuePage,
  NotificationQueueStatus,
  NotificationStatus,
  NotificationTemplateDetail,
  NotificationTemplateInfo,
  NotificationTestDelivery,
  ObjectReference,
  Order,
  OrderPage,
  OrderStatus,
  PluginBlockDefinition,
  PluginInfo,
  PluginOrder,
  PreviewRequest,
  PreviewResult,
  ProductVariant,
  RefundOrderPayload,
  RegistrationPayload,
  RegistrationResult,
  SalesReport,
  ScheduleItem,
  ScheduleResponse,
  SchemaDefinition,
  SeoSettings,
  SessionInfo,
  ShopConfig,
  SiteSettings,
  SystemSettingsResponse,
  UpdateSystemSettings,
  StorageMeta,
  StorageObjectPage,
  StorageObjectQuery,
  ThemeInfo,
  UpdateForumCategoryPayload,
  UpdateHeaderSettings,
  UpdateSeoSettingsPayload,
  Enable2faResult,
  LoginResponse,
  TotpSetupResult,
  UpdateUserPayload,
  UpsertVariantPayload,
  UserAccount,
} from '../types';
import { getSessionToken } from './session';

/**
 * Base URL of the AegisCMS server (origin plus `[server].base_path`, without a trailing slash),
 * which serves the API, the public site, theme assets and uploads. `VITE_AEGIS_SERVER_URL` at
 * build time wins (Admin Studio hosted elsewhere, e.g. `https://admin.example.com` talking to
 * `https://example.com/cms`). Otherwise the dev server talks to a local core server, and the
 * production build to the server that served it: the bundle lives at
 * `{server}/admin/assets/*.js`, so it works behind a reverse proxy on any domain and sub-path
 * without rebuilding.
 */
const resolveServerUrl = (): string => {
  const configured = import.meta.env.VITE_AEGIS_SERVER_URL?.trim();
  if (configured) return new URL(configured).href.replace(/\/+$/, '');
  if (import.meta.env.DEV) return 'http://127.0.0.1:8080';
  // Held in a variable so Vite does not treat `new URL(..., import.meta.url)` as an asset import
  const moduleUrl = import.meta.url;
  return new URL('../../', moduleUrl).href.replace(/\/+$/, '');
};

export const SERVER_URL = resolveServerUrl();

const API_BASE = `${SERVER_URL}/api/v1`;

/**
 * Absolute URL of a server path such as an avatar (`/uploads/avatars/...`, which already
 * includes `[server].base_path`), so it also loads when Admin Studio runs on another origin
 */
export const serverAssetUrl = (url: string): string =>
  url.startsWith('/') && !url.startsWith('//') ? `${new URL(SERVER_URL).origin}${url}` : url;

export class ApiError extends Error {
  status: number;
  /** Seconds until the request may be retried (`retry_after_secs` of a 429 response) */
  retryAfterSecs?: number;
  /** Machine-readable reason (`code`), sent by some endpoints such as registration */
  code?: string;
  constructor(status: number, message: string, retryAfterSecs?: number, code?: string) {
    super(message);
    this.status = status;
    this.retryAfterSecs = retryAfterSecs;
    this.code = code;
  }
}

/** Turns a failed unauthenticated JSON request into an `ApiError` with the server's `code` */
const unauthenticatedError = async (res: Response, fallback: string): Promise<ApiError> => {
  const err = await res.json().catch(() => ({}));
  const retryAfter = typeof err.retry_after_secs === 'number' ? err.retry_after_secs : undefined;
  return new ApiError(res.status, err.error || fallback, retryAfter, err.code);
};

// Called when an authenticated request comes back 401 (expired or revoked session)
let unauthorizedHandler: (() => void) | null = null;

export const setUnauthorizedHandler = (handler: (() => void) | null) => {
  unauthorizedHandler = handler;
};

/** Search words and filters of `listContentSummaries` */
export interface ContentListParams {
  /** Partial match on the title, the slug and the excerpt */
  q?: string;
  /** A category id, or `none` for uncategorized */
  category?: string;
  status?: string;
  limit?: number;
  offset?: number;
}

export interface ContentSummaryPage {
  items: ContentSummary[];
  /** The number of items matching the search and filters (not only this page) */
  total: number;
  limit: number;
  offset: number;
}

const toQuery = (query: object): string => {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined && value !== '') params.set(key, String(value));
  }
  return params.toString();
};

/**
 * Fetches an API endpoint with the session token as `Authorization: Bearer`. A 401 notifies
 * the unauthorized handler (the app returns to the login screen) before the error is thrown.
 */
const request = async <T>(path: string, init: RequestInit, fallbackError: string): Promise<T> => {
  const token = getSessionToken();
  const headers = new Headers(init.headers);
  if (token) headers.set('Authorization', `Bearer ${token}`);
  const res = await fetch(`${API_BASE}${path}`, { ...init, headers });
  if (!res.ok) {
    if (res.status === 401 && token) unauthorizedHandler?.();
    const err = await res.json().catch(() => ({ error: fallbackError }));
    throw new ApiError(res.status, err.error || fallbackError);
  }
  return res.json();
};

/** `request` for endpoints that answer 204 No Content */
const requestNoContent = async (
  path: string,
  init: RequestInit,
  fallbackError: string
): Promise<void> => {
  const token = getSessionToken();
  const headers = new Headers(init.headers);
  if (token) headers.set('Authorization', `Bearer ${token}`);
  const res = await fetch(`${API_BASE}${path}`, { ...init, headers });
  if (res.ok) return;
  if (res.status === 401 && token) unauthorizedHandler?.();
  const err = await res.json().catch(() => ({ error: fallbackError }));
  throw new ApiError(res.status, err.error || fallbackError);
};

const JSON_HEADERS = { 'Content-Type': 'application/json' };

export const aegisApi = {
  async checkHealth(): Promise<{ status: string; system?: string; version?: string }> {
    const res = await fetch(`${API_BASE}/health`);
    if (!res.ok) throw new ApiError(res.status, i18n.t('api.healthFailed'));
    return res.json();
  },

  /**
   * Logs in with a username and password. A wrong password is an `ApiError` with status 401,
   * and too many failed attempts an `ApiError` with status 429 and `retryAfterSecs`
   */
  async login(username: string, password: string): Promise<LoginResponse> {
    const res = await fetch(`${API_BASE}/auth/session`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: i18n.t('api.loginFailed') }));
      const retryAfter =
        typeof err.retry_after_secs === 'number' ? err.retry_after_secs : undefined;
      throw new ApiError(res.status, err.error || i18n.t('api.loginFailed'), retryAfter);
    }
    return res.json();
  },

  /** Verifies a 2FA code (TOTP or backup code) for an MFA challenge */
  async verify2fa(challenge_token: string, code: string): Promise<SessionInfo> {
    const res = await fetch(`${API_BASE}/auth/session/verify-2fa`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ challenge_token, code }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Invalid verification code' }));
      throw new ApiError(res.status, err.error || 'Invalid verification code');
    }
    return res.json();
  },

  /** Sign-in methods for the login screen */
  /** Applies for an account; the server emails a confirmation link (202) */
  async register(payload: RegistrationPayload): Promise<{ expires_in_secs: number }> {
    const res = await fetch(`${API_BASE}/auth/registration`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.registerFailed'));
    return res.json();
  },

  /** Creates the account from the token of the confirmation link */
  async verifyRegistration(token: string): Promise<RegistrationResult> {
    const res = await fetch(`${API_BASE}/auth/registration/verify`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token }),
    });
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.verifyFailed'));
    return res.json();
  },

  /** Requests a password reset link by email (202 Accepted) */
  async requestPasswordReset(email: string): Promise<void> {
    const res = await fetch(`${API_BASE}/auth/password-reset`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
    });
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.passwordResetFailed'));
  },

  /** Sets a new password using the reset token */
  async confirmPasswordReset(token: string, password: string): Promise<void> {
    const res = await fetch(`${API_BASE}/auth/password-reset/confirm`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token, password }),
    });
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.confirmResetFailed'));
  },

  /** Creates an invitation link and emails it to the recipient (admin only) */
  async createInvitation(payload: CreateInvitationPayload): Promise<void> {
    return request(
      '/auth/invitations',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.createInvitationFailed')
    );
  },

  /** Gets information about an invitation token for the acceptance screen */
  async getInvitationInfo(token: string): Promise<InvitationInfo> {
    const res = await fetch(`${API_BASE}/auth/invitations/info?token=${encodeURIComponent(token)}`);
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.invitationInfoFailed'));
    return res.json();
  },

  /** Accepts an invitation, setting the initial username and password */
  async acceptInvitation(
    payload: AcceptInvitePayload
  ): Promise<{ status: string; username: string }> {
    const res = await fetch(`${API_BASE}/auth/invitations/accept`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    if (!res.ok) throw await unauthenticatedError(res, i18n.t('api.acceptInvitationFailed'));
    return res.json();
  },

  async getAuthProviders(): Promise<AuthProviders> {
    const res = await fetch(`${API_BASE}/auth/providers`);
    if (!res.ok) throw new ApiError(res.status, i18n.t('api.providersFailed'));
    return res.json();
  },

  /** The session for the stored token, or null when it is missing, expired or revoked */
  async getCurrentSession(): Promise<SessionInfo | null> {
    const token = getSessionToken();
    if (!token) return null;
    const res = await fetch(`${API_BASE}/auth/session`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (res.status === 401) return null;
    if (!res.ok) throw new ApiError(res.status, i18n.t('api.sessionFailed'));
    return res.json();
  },

  async logout(): Promise<void> {
    await request('/auth/session', { method: 'DELETE' }, i18n.t('api.logoutFailed'));
  },

  async getSchemas(): Promise<SchemaDefinition[]> {
    const data = await request<{ schemas?: SchemaDefinition[] }>(
      '/schemas',
      {},
      i18n.t('api.schemasFailed')
    );
    return data.schemas || [];
  },

  /**
   * One page of content summaries (no blocks and no rendered HTML, with an `excerpt`) matching the
   * search words and filters. `total` counts every match, not only this page.
   * `category` is a category id, or `none` for uncategorized.
   */
  async listContentSummaries(
    contentType: string,
    params: ContentListParams = {}
  ): Promise<ContentSummaryPage> {
    const data = await request<{
      contents?: ContentSummary[];
      total?: number;
      limit?: number;
      offset?: number;
    }>(
      `/contents/${encodeURIComponent(contentType)}?${toQuery({ ...params, view: 'summary' })}`,
      {},
      i18n.t('api.contentsFailed')
    );
    return {
      items: data.contents ?? [],
      total: data.total ?? 0,
      limit: data.limit ?? params.limit ?? 0,
      offset: data.offset ?? params.offset ?? 0,
    };
  },

  /** Every item of the content type as summaries (for small lists such as the categories) */
  async getAllContentSummaries(contentType: string): Promise<ContentSummary[]> {
    const pageSize = 500;
    const items: ContentSummary[] = [];
    for (;;) {
      const page = await aegisApi.listContentSummaries(contentType, {
        limit: pageSize,
        offset: items.length,
      });
      items.push(...page.items);
      if (page.items.length < pageSize || items.length >= page.total) return items;
    }
  },

  /**
   * Every item of the content type, with blocks and rendered HTML (the list API returns at most 500
   * per page). Prefer `listContentSummaries`: this renders every item on the server
   */
  async getContents(contentType: string): Promise<ContentItem[]> {
    const pageSize = 500;
    const items: ContentItem[] = [];
    for (;;) {
      const data = await request<{ contents?: ContentItem[]; total?: number }>(
        `/contents/${contentType}?${toQuery({ limit: pageSize, offset: items.length })}`,
        {},
        i18n.t('api.contentsFailed')
      );
      const page = data.contents || [];
      items.push(...page);
      if (page.length < pageSize || items.length >= (data.total ?? 0)) return items;
    }
  },

  /** Scheduled and published content across schemas for the calendar view */
  async getSchedule(contentType?: string): Promise<ScheduleItem[]> {
    const query = contentType ? `?content_type=${encodeURIComponent(contentType)}` : '';
    const res = await request<ScheduleResponse>(
      `/schedule${query}`,
      {},
      i18n.t('api.contentsFailed')
    );
    return res.items || [];
  },

  async getContent(contentType: string, slugOrId: string): Promise<ContentItem> {
    return request(`/contents/${contentType}/${slugOrId}`, {}, i18n.t('api.contentNotFound'));
  },

  async createCategory(payload: CreateCategoryPayload): Promise<ContentItem> {
    return this.createContent('category', payload);
  },

  async createContent(
    contentType: string,
    payload: CreateContentPayload | CreateCategoryPayload
  ): Promise<ContentItem> {
    return request(
      `/contents/${contentType}`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.saveContentFailed')
    );
  },

  /** Overwrites a saved content item (same body as create; plugins run again) */
  async updateContent(
    contentType: string,
    idOrSlug: string,
    payload: CreateContentPayload | CreateCategoryPayload | Record<string, unknown>
  ): Promise<ContentItem> {
    return request(
      `/contents/${contentType}/${encodeURIComponent(idOrSlug)}`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.saveContentFailed')
    );
  },

  async listContentVersions(
    contentType: string,
    idOrSlug: string
  ): Promise<
    { revision: number; created_at: string; created_by?: string; change_note?: string }[]
  > {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(idOrSlug)}/versions`,
      {},
      i18n.t('api.contentVersionsFailed')
    );
  },

  async restoreContentVersion(
    contentType: string,
    idOrSlug: string,
    revision: number
  ): Promise<ContentItem> {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(idOrSlug)}/versions/${revision}/restore`,
      { method: 'POST' },
      i18n.t('api.restoreContentVersionFailed')
    );
  },

  async deleteContent(contentType: string, idOrSlug: string): Promise<void> {
    const token = getSessionToken();
    const headers = new Headers();
    if (token) headers.set('Authorization', `Bearer ${token}`);
    const res = await fetch(`${API_BASE}/contents/${contentType}/${encodeURIComponent(idOrSlug)}`, {
      method: 'DELETE',
      headers,
    });
    if (res.ok) return;
    if (res.status === 401 && token) unauthorizedHandler?.();
    const err = await res.json().catch(() => ({ error: i18n.t('api.deleteContentFailed') }));
    throw new ApiError(res.status, err.error || i18n.t('api.deleteContentFailed'));
  },

  async listVariants(contentType: string, productId: string): Promise<ProductVariant[]> {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(productId)}/variants`,
      {},
      i18n.t('api.variantsFailed')
    );
  },

  async upsertVariant(
    contentType: string,
    productId: string,
    payload: UpsertVariantPayload
  ): Promise<ProductVariant> {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(productId)}/variants`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.saveVariantFailed')
    );
  },

  async deleteVariant(contentType: string, productId: string, variantId: string): Promise<void> {
    const token = getSessionToken();
    const headers = new Headers();
    if (token) headers.set('Authorization', `Bearer ${token}`);
    const res = await fetch(
      `${API_BASE}/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(productId)}/variants/${encodeURIComponent(variantId)}`,
      {
        method: 'DELETE',
        headers,
      }
    );
    if (res.ok) return;
    if (res.status === 401 && token) unauthorizedHandler?.();
    const err = await res.json().catch(() => ({ error: i18n.t('api.deleteVariantFailed') }));
    throw new ApiError(res.status, err.error || i18n.t('api.deleteVariantFailed'));
  },

  async getShopConfig(): Promise<ShopConfig> {
    return request('/shop/config', {}, i18n.t('api.shopConfigFailed'));
  },

  /** Readers' comments, newest first, with counts per status (404 when [comments] is disabled) */
  async listComments(query: {
    status?: CommentStatus | '';
    limit?: number;
    offset?: number;
  }): Promise<CommentPage> {
    return request(`/comments?${toQuery(query)}`, {}, i18n.t('api.commentsFailed'));
  },

  /** Approves, holds, marks as spam or trashes a comment */
  async moderateComment(id: string, status: CommentStatus): Promise<void> {
    return requestNoContent(
      `/comments/${encodeURIComponent(id)}`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ status }) },
      i18n.t('api.moderateCommentFailed')
    );
  },

  /** Rewrites the text of a comment (any status) */
  async editComment(id: string, body: string): Promise<void> {
    return requestNoContent(
      `/comments/${encodeURIComponent(id)}`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ body }) },
      i18n.t('api.editCommentFailed')
    );
  },

  /** Reads whether new comments are open on one content item */
  async getContentCommentsOpen(contentType: string, contentId: string): Promise<{ open: boolean }> {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(contentId)}/comments/state`,
      {},
      i18n.t('api.contentCommentsFailed')
    );
  },

  /** Stops (or resumes) new comments on one content; the comments already there stay */
  async setContentCommentsOpen(
    contentType: string,
    contentId: string,
    open: boolean
  ): Promise<{ open: boolean }> {
    return request(
      `/contents/${encodeURIComponent(contentType)}/${encodeURIComponent(contentId)}/comments/state`,
      { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify({ open }) },
      i18n.t('api.contentCommentsFailed')
    );
  },

  /** Deletes a comment permanently */
  async deleteComment(id: string): Promise<void> {
    return requestNoContent(
      `/comments/${encodeURIComponent(id)}`,
      { method: 'DELETE' },
      i18n.t('api.deleteCommentFailed')
    );
  },

  /** Changes the status of (or deletes) several comments; returns how many changed */
  async bulkComments(ids: string[], action: CommentBulkAction): Promise<{ count: number }> {
    return request(
      '/comments/bulk',
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ ids, action }) },
      i18n.t('api.moderateCommentFailed')
    );
  },

  /** Replies as the signed-in user (published right away; approves the comment if it waits) */
  async replyToComment(id: string, body: string): Promise<{ id: string }> {
    return request(
      `/comments/${encodeURIComponent(id)}/replies`,
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ body }) },
      i18n.t('api.replyCommentFailed')
    );
  },

  /** Access analytics for `from`..`to` (`YYYY-MM-DD`, UTC); 404 when [analytics] is disabled */
  async getAnalyticsSummary(query: {
    from: string;
    to: string;
    limit?: number;
  }): Promise<AnalyticsSummary> {
    return request(`/analytics/summary?${toQuery(query)}`, {}, i18n.t('api.analyticsFailed'));
  },

  /** Approval settings (404 when [comments] is disabled) */
  async getCommentSettings(): Promise<CommentSettings> {
    return request('/comments/settings', {}, i18n.t('api.commentSettingsFailed'));
  },

  /** Changes the approval settings (admin); `null` returns to the configuration file's value */
  async updateCommentSettings(overrides: CommentSettings['overrides']): Promise<CommentSettings> {
    return request(
      '/comments/settings',
      { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify(overrides) },
      i18n.t('api.updateCommentSettingsFailed')
    );
  },

  /** Contact forms and their fields (404 when [contact] is disabled) */
  async listContactForms(): Promise<ContactForm[]> {
    return request('/contact/forms', {}, i18n.t('api.contactFormsFailed'));
  },

  /** The contact inbox, newest first, with counts per status */
  async listContactMessages(query: {
    status?: ContactStatus | '';
    form?: string;
    limit?: number;
    offset?: number;
  }): Promise<ContactMessagePage> {
    return request(`/contact/messages?${toQuery(query)}`, {}, i18n.t('api.contactMessagesFailed'));
  },

  /** Changes a message's status (`new` / `read` / `done` / `spam`) */
  async updateContactMessage(id: string, status: ContactStatus): Promise<void> {
    return requestNoContent(
      `/contact/messages/${encodeURIComponent(id)}`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ status }) },
      i18n.t('api.updateContactMessageFailed')
    );
  },

  /** Changes the status of (or deletes) several messages; returns how many changed */
  async bulkContactMessages(
    ids: string[],
    action: ContactStatus | 'delete'
  ): Promise<{ count: number }> {
    return request(
      '/contact/messages/bulk',
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ ids, action }) },
      i18n.t('api.updateContactMessageFailed')
    );
  },

  async deleteContactMessage(id: string): Promise<void> {
    return requestNoContent(
      `/contact/messages/${encodeURIComponent(id)}`,
      { method: 'DELETE' },
      i18n.t('api.deleteContactMessageFailed')
    );
  },

  /** Shop orders, newest first (404 when [commerce] is disabled) */
  async listOrders(query: {
    status?: OrderStatus | '';
    limit?: number;
    offset?: number;
  }): Promise<OrderPage> {
    return request(`/orders?${toQuery(query)}`, {}, i18n.t('api.ordersFailed'));
  },

  async getSalesReport(start: string, end: string): Promise<SalesReport> {
    return request(
      `/orders/report?${toQuery({ start, end })}`,
      {},
      i18n.t('api.salesReportFailed')
    );
  },

  async getOrder(id: string): Promise<Order> {
    return request(`/orders/${encodeURIComponent(id)}`, {}, i18n.t('api.orderFailed'));
  },

  /** Marks an order paid / shipped / completed / cancelled (cancelling before shipping returns stock) */
  async updateOrderStatus(id: string, status: OrderStatus, note?: string): Promise<Order> {
    return request(
      `/orders/${encodeURIComponent(id)}/status`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status, note }),
      },
      i18n.t('api.updateOrderFailed')
    );
  },

  /** Refunds an order partially or fully (supports online gateway or manual record, optionally restocks items) */
  async refundOrder(id: string, payload: RefundOrderPayload): Promise<Order> {
    return request(
      `/orders/${encodeURIComponent(id)}/refund`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.refundOrderFailed')
    );
  },

  async getAuditLogs(query: AuditLogQuery): Promise<AuditLogPage> {
    return request(`/audit-logs?${toQuery(query)}`, {}, i18n.t('api.auditFailed'));
  },

  async listStorageObjects(query: StorageObjectQuery): Promise<StorageObjectPage> {
    return request(`/storage/objects?${toQuery(query)}`, {}, i18n.t('api.mediaFailed'));
  },

  /** Renders unsaved content with the server's save + publish pipeline (nothing is stored) */
  /** Custom blocks of the enabled plugins */
  async listBlocks(): Promise<PluginBlockDefinition[]> {
    const data = await request<{ blocks?: PluginBlockDefinition[] }>(
      '/blocks',
      {},
      i18n.t('api.blocksFailed')
    );
    return data.blocks || [];
  },

  async previewContent(preview: PreviewRequest): Promise<PreviewResult> {
    return request(
      '/preview',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(preview),
      },
      i18n.t('api.previewFailed')
    );
  },

  /**
   * Deletes a stored file. When saved contents still use it and `force` is false, nothing is
   * deleted and the result lists them (`deleted: false`)
   */
  async deleteStorageObject(
    key: string,
    force = false
  ): Promise<{ deleted: boolean; references: ObjectReference[] }> {
    const path = key.split('/').map(encodeURIComponent).join('/');
    const token = getSessionToken();
    const headers = new Headers();
    if (token) headers.set('Authorization', `Bearer ${token}`);
    const res = await fetch(`${API_BASE}/storage/objects/${path}${force ? '?force=true' : ''}`, {
      method: 'DELETE',
      headers,
    });
    if (res.ok) return { deleted: true, references: [] };
    const err = await res.json().catch(() => ({ error: i18n.t('api.deleteFileFailed') }));
    if (res.status === 409 && Array.isArray(err.references)) {
      return { deleted: false, references: err.references };
    }
    if (res.status === 401 && token) unauthorizedHandler?.();
    throw new ApiError(res.status, err.error || i18n.t('api.deleteFileFailed'));
  },

  async uploadFile(file: File): Promise<StorageMeta> {
    const form = new FormData();
    form.append('file', file);
    return request('/storage/upload', { method: 'POST', body: form }, i18n.t('api.uploadFailed'));
  },

  /** Every user account (admin only) */
  async listUsers(): Promise<UserAccount[]> {
    const data = await request<{ users?: UserAccount[] }>('/users', {}, i18n.t('api.usersFailed'));
    return data.users || [];
  },

  /** The logged-in user's own account, or any account for admins */
  async getUser(username: string): Promise<UserAccount> {
    return request(`/users/${encodeURIComponent(username)}`, {}, i18n.t('api.userFailed'));
  },

  async createUser(payload: CreateUserPayload): Promise<UserAccount> {
    return request(
      '/users',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.createUserFailed')
    );
  },

  async updateUser(username: string, payload: UpdateUserPayload): Promise<UserAccount> {
    return request(
      `/users/${encodeURIComponent(username)}`,
      {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.updateUserFailed')
    );
  },

  async deleteUser(username: string): Promise<void> {
    const token = getSessionToken();
    const headers = new Headers();
    if (token) headers.set('Authorization', `Bearer ${token}`);
    const res = await fetch(`${API_BASE}/users/${encodeURIComponent(username)}`, {
      method: 'DELETE',
      headers,
    });
    if (res.ok) return;
    if (res.status === 401 && token) unauthorizedHandler?.();
    const err = await res.json().catch(() => ({ error: i18n.t('api.deleteUserFailed') }));
    throw new ApiError(res.status, err.error || i18n.t('api.deleteUserFailed'));
  },

  /** Sets the avatar (PNG, JPEG, GIF or WebP up to 2 MB) of the user */
  async uploadAvatar(username: string, file: File): Promise<UserAccount> {
    const form = new FormData();
    form.append('file', file);
    return request(
      `/users/${encodeURIComponent(username)}/avatar`,
      { method: 'PUT', body: form },
      i18n.t('api.uploadAvatarFailed')
    );
  },

  /** Initiates 2FA setup for the current user (returns secret, otpauth URL and QR code) */
  async setup2fa(): Promise<TotpSetupResult> {
    return request('/users/me/2fa/setup', { method: 'POST' }, 'Failed to set up 2FA');
  },

  /** Confirms 2FA setup with a code and returns the 6 one-time backup codes */
  async enable2fa(secret: string, code: string): Promise<Enable2faResult> {
    return request(
      '/users/me/2fa/enable',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ secret, code }),
      },
      'Failed to enable 2FA'
    );
  },

  /** Disables 2FA for the current user */
  async disable2fa(payload: { password?: string; code?: string }): Promise<{ enabled: boolean }> {
    return request(
      '/users/me/2fa/disable',
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      'Failed to disable 2FA'
    );
  },

  /** Admin operation: forcibly resets a user\'s 2FA */
  async resetUser2fa(username: string): Promise<{ success: boolean; username: string }> {
    return request(
      `/users/${encodeURIComponent(username)}/2fa/reset`,
      { method: 'POST' },
      'Failed to reset user 2FA'
    );
  },

  async deleteAvatar(username: string): Promise<UserAccount> {
    return request(
      `/users/${encodeURIComponent(username)}/avatar`,
      { method: 'DELETE' },
      i18n.t('api.removeAvatarFailed')
    );
  },

  async listThemes(): Promise<ThemeInfo[]> {
    return request('/themes', {}, i18n.t('api.themesFailed'));
  },

  async getActiveTheme(): Promise<ThemeInfo> {
    return request('/themes/active', {}, i18n.t('api.activeThemeFailed'));
  },

  async switchActiveTheme(themeId: string): Promise<ThemeInfo> {
    return request(
      '/themes/active',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ theme_id: themeId }),
      },
      i18n.t('api.switchThemeFailed')
    );
  },

  async installTheme(file: File): Promise<ThemeInfo> {
    const form = new FormData();
    form.append('file', file);
    return request(
      '/themes/install',
      { method: 'POST', body: form },
      i18n.t('api.installThemeFailed')
    );
  },

  async listPlugins(): Promise<PluginInfo[]> {
    return request('/plugins', {}, i18n.t('api.pluginsFailed'));
  },

  async installPlugin(file: File): Promise<PluginInfo> {
    const form = new FormData();
    form.append('file', file);
    return request(
      '/plugins/install',
      { method: 'POST', body: form },
      i18n.t('api.installPluginFailed')
    );
  },

  async getPluginSettings(name: string): Promise<Record<string, string>> {
    const res = await request<{ settings: Record<string, string> }>(
      `/plugins/${encodeURIComponent(name)}/settings`,
      {},
      i18n.t('api.getPluginSettingsFailed')
    );
    return res.settings;
  },

  async updatePluginSettings(
    name: string,
    settings: Record<string, string>
  ): Promise<Record<string, string>> {
    const res = await request<{ settings: Record<string, string> }>(
      `/plugins/${encodeURIComponent(name)}/settings`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ settings }),
      },
      i18n.t('api.updatePluginSettingsFailed')
    );
    return res.settings;
  },

  /** Enables or disables a plugin without a restart; returns the updated plugin list */
  async setPluginEnabled(name: string, enabled: boolean): Promise<PluginInfo[]> {
    return request(
      `/plugins/${encodeURIComponent(name)}/enabled`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      },
      i18n.t('api.setPluginEnabledFailed')
    );
  },

  async getPluginOrder(): Promise<PluginOrder> {
    return request('/plugins/order', {}, i18n.t('api.pluginOrderFailed'));
  },

  /** Reorders the enabled plugins (each exactly once) without a restart */
  async setPluginOrder(enabled: string[]): Promise<PluginOrder> {
    return request(
      '/plugins/order',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      },
      i18n.t('api.setPluginOrderFailed')
    );
  },

  /** Drops the list saved from Admin Studio: `[plugins].enabled` of the configuration file applies */
  async resetPluginOrder(): Promise<PluginOrder> {
    return request('/plugins/order', { method: 'DELETE' }, i18n.t('api.resetPluginOrderFailed'));
  },

  async getSystemSettings(): Promise<SystemSettingsResponse> {
    return request('/system/settings', {}, i18n.t('api.systemSettingsFailed'));
  },

  async updateSystemSettings(settings: UpdateSystemSettings): Promise<SystemSettingsResponse> {
    return request(
      '/system/settings',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
      },
      i18n.t('api.updateSystemSettingsFailed')
    );
  },

  async getSiteSettings(): Promise<SiteSettings> {
    return request('/site', {}, i18n.t('api.siteSettingsFailed'));
  },

  /** null (or an empty string) goes back to the configuration file's value */
  async updateSiteSettings(settings: {
    title: string | null;
    favicon: string | null;
  }): Promise<SiteSettings> {
    return request(
      '/site',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
      },
      i18n.t('api.updateSiteSettingsFailed')
    );
  },

  async getSeoSettings(): Promise<SeoSettings> {
    return request('/seo', {}, i18n.t('api.seoSettingsFailed'));
  },

  async updateSeoSettings(payload: UpdateSeoSettingsPayload): Promise<SeoSettings> {
    return request(
      '/seo',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.updateSeoSettingsFailed')
    );
  },

  async getHeadersSettings(): Promise<HeaderSettings> {
    return request('/headers', {}, i18n.t('api.headersSettingsFailed'));
  },

  async updateHeadersSettings(payload: UpdateHeaderSettings): Promise<HeaderSettings> {
    return request(
      '/headers',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      },
      i18n.t('api.updateHeadersSettingsFailed')
    );
  },

  /** Notification setup and queue counts (`enabled: false` when [notifications] is off) */
  async getNotificationStatus(): Promise<NotificationStatus> {
    return request('/notifications', {}, i18n.t('api.notificationStatusFailed'));
  },

  async listNotificationQueue(query: {
    status?: NotificationQueueStatus;
    limit?: number;
    offset?: number;
  }): Promise<NotificationQueuePage> {
    const params = new URLSearchParams();
    if (query.status) params.set('status', query.status);
    if (query.limit) params.set('limit', String(query.limit));
    if (query.offset) params.set('offset', String(query.offset));
    return request(
      `/notifications/queue?${params.toString()}`,
      {},
      i18n.t('api.notificationQueueFailed')
    );
  },

  /** Queues a failed notification again from the first attempt */
  async retryNotification(id: string): Promise<void> {
    return requestNoContent(
      `/notifications/queue/${encodeURIComponent(id)}/retry`,
      { method: 'POST' },
      i18n.t('api.retryNotificationFailed')
    );
  },

  async discardNotification(id: string): Promise<void> {
    return requestNoContent(
      `/notifications/queue/${encodeURIComponent(id)}`,
      { method: 'DELETE' },
      i18n.t('api.discardNotificationFailed')
    );
  },

  async listNotificationTemplates(): Promise<NotificationTemplateInfo[]> {
    return request('/notifications/templates', {}, i18n.t('api.notificationTemplatesFailed'));
  },

  async getNotificationTemplate(name: string): Promise<NotificationTemplateDetail> {
    return request(
      `/notifications/templates/${encodeURIComponent(name)}`,
      {},
      i18n.t('api.notificationTemplatesFailed')
    );
  },

  async saveNotificationTemplate(
    name: string,
    content: string
  ): Promise<NotificationTemplateDetail> {
    return request(
      `/notifications/templates/${encodeURIComponent(name)}`,
      { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify({ content }) },
      i18n.t('api.saveNotificationTemplateFailed')
    );
  },

  /** Drops the edit: the `templates_dir` file or the built-in template applies again */
  async resetNotificationTemplate(name: string): Promise<NotificationTemplateDetail> {
    return request(
      `/notifications/templates/${encodeURIComponent(name)}`,
      { method: 'DELETE' },
      i18n.t('api.saveNotificationTemplateFailed')
    );
  },

  /** Renders a notification with sample values, using `templates` (unsaved edits) where given */
  async previewNotification(payload: {
    kind: string;
    staff: boolean;
    templates: Record<string, string>;
  }): Promise<NotificationPreview> {
    return request(
      '/notifications/preview',
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify(payload) },
      i18n.t('api.previewNotificationFailed')
    );
  },

  /** Sends a notification with sample values right away (no retries) and returns the results */
  async sendTestNotification(payload: {
    kind: string;
    to: string | null;
    staff: boolean;
  }): Promise<NotificationTestDelivery[]> {
    return request(
      '/notifications/test',
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify(payload) },
      i18n.t('api.testNotificationFailed')
    );
  },

  // --- Forum (掲示板モデレーション) ---

  async listForumCategories(): Promise<ForumCategory[]> {
    const data = await request<{ categories: ForumCategory[] }>(
      '/forum/categories',
      {},
      'Failed to load forum categories'
    );
    return data.categories;
  },

  async createForumCategory(payload: CreateForumCategoryPayload): Promise<ForumCategory> {
    const data = await request<{ category: ForumCategory }>(
      '/forum/categories',
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify(payload) },
      'Failed to create forum category'
    );
    return data.category;
  },

  async updateForumCategory(
    idOrSlug: string,
    payload: UpdateForumCategoryPayload
  ): Promise<ForumCategory> {
    const data = await request<{ category: ForumCategory }>(
      `/forum/categories/${encodeURIComponent(idOrSlug)}`,
      { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify(payload) },
      'Failed to update forum category'
    );
    return data.category;
  },

  async deleteForumCategory(idOrSlug: string): Promise<void> {
    await request<void>(
      `/forum/categories/${encodeURIComponent(idOrSlug)}`,
      { method: 'DELETE' },
      'Failed to delete forum category'
    );
  },

  async listForumThreads(params?: {
    category_slug?: string;
    status?: string;
    limit?: number;
    cursor_id?: string;
    cursor_last_post_at?: string;
  }): Promise<ForumThread[]> {
    const sp = new URLSearchParams();
    if (params?.category_slug) sp.set('category_slug', params.category_slug);
    if (params?.status) sp.set('status', params.status);
    if (params?.limit) sp.set('limit', String(params.limit));
    if (params?.cursor_id) sp.set('cursor_id', params.cursor_id);
    if (params?.cursor_last_post_at) sp.set('cursor_last_post_at', params.cursor_last_post_at);
    const qs = sp.toString() ? `?${sp.toString()}` : '';

    const data = await request<{ threads: ForumThread[] }>(
      `/forum/threads${qs}`,
      {},
      'Failed to load forum threads'
    );
    return data.threads;
  },

  async pinForumThread(threadId: string, pinned: boolean): Promise<void> {
    await request<void>(
      `/forum/threads/${encodeURIComponent(threadId)}/pin`,
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ pinned }) },
      'Failed to update thread pin status'
    );
  },

  async lockForumThread(threadId: string, locked: boolean): Promise<void> {
    await request<void>(
      `/forum/threads/${encodeURIComponent(threadId)}/lock`,
      { method: 'POST', headers: JSON_HEADERS, body: JSON.stringify({ locked }) },
      'Failed to update thread lock status'
    );
  },

  async moderateForumThread(threadId: string, status: CommentStatus): Promise<ForumThread> {
    const data = await request<{ thread: ForumThread }>(
      `/forum/threads/${encodeURIComponent(threadId)}/status`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ status }) },
      'Failed to moderate forum thread'
    );
    return data.thread;
  },

  async deleteForumThread(threadId: string): Promise<void> {
    await request<void>(
      `/forum/threads/${encodeURIComponent(threadId)}`,
      { method: 'DELETE' },
      'Failed to delete forum thread'
    );
  },

  async listForumPosts(
    threadId: string,
    params?: { limit?: number; cursor_post_number?: number; status?: string }
  ): Promise<ForumPost[]> {
    const sp = new URLSearchParams();
    if (params?.limit) sp.set('limit', String(params.limit));
    if (params?.cursor_post_number) sp.set('cursor_post_number', String(params.cursor_post_number));
    if (params?.status) sp.set('status', params.status);
    const qs = sp.toString() ? `?${sp.toString()}` : '';

    const data = await request<{ posts: ForumPost[] }>(
      `/forum/threads/${encodeURIComponent(threadId)}/posts${qs}`,
      {},
      'Failed to load forum posts'
    );
    return data.posts;
  },

  async moderateForumPost(postId: string, status: CommentStatus): Promise<ForumPost> {
    const data = await request<{ post: ForumPost }>(
      `/forum/posts/${encodeURIComponent(postId)}/status`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ status }) },
      'Failed to moderate forum post'
    );
    return data.post;
  },

  async deleteForumPost(postId: string): Promise<void> {
    await request<void>(
      `/forum/posts/${encodeURIComponent(postId)}`,
      { method: 'DELETE' },
      'Failed to delete forum post'
    );
  },

  async getForumSettings(): Promise<ForumModerationSettings> {
    return request<ForumModerationSettings>(
      '/forum/settings',
      {},
      'Failed to load forum moderation settings'
    );
  },

  async updateForumSettings(payload: {
    require_approval: boolean | null;
    auto_approve_known: boolean | null;
  }): Promise<ForumModerationSettings> {
    return request<ForumModerationSettings>(
      '/forum/settings',
      { method: 'PUT', headers: JSON_HEADERS, body: JSON.stringify(payload) },
      'Failed to update forum moderation settings'
    );
  },

  async listForumReports(params?: { status?: string; limit?: number }): Promise<ForumReport[]> {
    const sp = new URLSearchParams();
    if (params?.status) sp.set('status', params.status);
    if (params?.limit) sp.set('limit', String(params.limit));
    const qs = sp.toString() ? `?${sp.toString()}` : '';
    const data = await request<{ reports: ForumReport[] }>(
      `/forum/reports${qs}`,
      {},
      'Failed to load forum reports'
    );
    return data.reports;
  },

  async resolveForumReport(
    reportId: string,
    status: 'open' | 'resolved' | 'dismissed'
  ): Promise<ForumReport> {
    const data = await request<{ report: ForumReport }>(
      `/forum/reports/${encodeURIComponent(reportId)}`,
      { method: 'PATCH', headers: JSON_HEADERS, body: JSON.stringify({ status }) },
      'Failed to update forum report status'
    );
    return data.report;
  },
};
