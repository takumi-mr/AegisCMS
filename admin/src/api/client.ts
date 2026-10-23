import i18n from '../i18n';
import type {
  AuditLogPage,
  AuthProviders,
  AuditLogQuery,
  ContentItem,
  CreateCategoryPayload,
  CreateContentPayload,
  CreateUserPayload,
  ObjectReference,
  Order,
  OrderPage,
  OrderStatus,
  PluginInfo,
  PreviewRequest,
  PreviewResult,
  RegistrationPayload,
  RegistrationResult,
  SchemaDefinition,
  SessionInfo,
  ShopConfig,
  StorageMeta,
  StorageObjectPage,
  StorageObjectQuery,
  ThemeInfo,
  UpdateUserPayload,
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

export const aegisApi = {
  async checkHealth(): Promise<{ status: string; system: string; version: string }> {
    const res = await fetch(`${API_BASE}/health`);
    if (!res.ok) throw new ApiError(res.status, i18n.t('api.healthFailed'));
    return res.json();
  },

  /**
   * Logs in with a username and password. A wrong password is an `ApiError` with status 401,
   * and too many failed attempts an `ApiError` with status 429 and `retryAfterSecs`
   */
  async login(username: string, password: string): Promise<SessionInfo> {
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

  /** Every item of the content type (the list API returns at most 500 per page) */
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

  async getShopConfig(): Promise<ShopConfig> {
    return request('/shop/config', {}, i18n.t('api.shopConfigFailed'));
  },

  /** Shop orders, newest first (404 when [commerce] is disabled) */
  async listOrders(query: {
    status?: OrderStatus | '';
    limit?: number;
    offset?: number;
  }): Promise<OrderPage> {
    return request(`/orders?${toQuery(query)}`, {}, i18n.t('api.ordersFailed'));
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

  async getAuditLogs(query: AuditLogQuery): Promise<AuditLogPage> {
    return request(`/audit-logs?${toQuery(query)}`, {}, i18n.t('api.auditFailed'));
  },

  async listStorageObjects(query: StorageObjectQuery): Promise<StorageObjectPage> {
    return request(`/storage/objects?${toQuery(query)}`, {}, i18n.t('api.mediaFailed'));
  },

  /** Renders unsaved content with the server's save + publish pipeline (nothing is stored) */
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

  async listPlugins(): Promise<PluginInfo[]> {
    return request('/plugins', {}, i18n.t('api.pluginsFailed'));
  },

  async getPluginSettings(name: string): Promise<Record<string, string>> {
    return request(
      `/plugins/${encodeURIComponent(name)}/settings`,
      {},
      i18n.t('api.getPluginSettingsFailed')
    );
  },

  async updatePluginSettings(
    name: string,
    settings: Record<string, string>
  ): Promise<Record<string, string>> {
    return request(
      `/plugins/${encodeURIComponent(name)}/settings`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
      },
      i18n.t('api.updatePluginSettingsFailed')
    );
  },
};
