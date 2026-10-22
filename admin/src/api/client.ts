import type {
  AuditLogPage,
  AuthProviders,
  AuditLogQuery,
  ContentItem,
  CreateCategoryPayload,
  CreateContentPayload,
  ObjectReference,
  PreviewRequest,
  PreviewResult,
  SchemaDefinition,
  SessionInfo,
  StorageMeta,
  StorageObjectPage,
  StorageObjectQuery,
  ThemeInfo,
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

export class ApiError extends Error {
  status: number;
  /** Seconds until the request may be retried (`retry_after_secs` of a 429 response) */
  retryAfterSecs?: number;
  constructor(status: number, message: string, retryAfterSecs?: number) {
    super(message);
    this.status = status;
    this.retryAfterSecs = retryAfterSecs;
  }
}

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
    if (!res.ok) throw new ApiError(res.status, 'Health check failed');
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
      const err = await res.json().catch(() => ({ error: 'Failed to log in' }));
      const retryAfter =
        typeof err.retry_after_secs === 'number' ? err.retry_after_secs : undefined;
      throw new ApiError(res.status, err.error || 'Failed to log in', retryAfter);
    }
    return res.json();
  },

  /** Sign-in methods for the login screen */
  async getAuthProviders(): Promise<AuthProviders> {
    const res = await fetch(`${API_BASE}/auth/providers`);
    if (!res.ok) throw new ApiError(res.status, 'Failed to fetch sign-in methods');
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
    if (!res.ok) throw new ApiError(res.status, 'Failed to check the session');
    return res.json();
  },

  async logout(): Promise<void> {
    await request('/auth/session', { method: 'DELETE' }, 'Failed to log out');
  },

  async getSchemas(): Promise<SchemaDefinition[]> {
    const data = await request<{ schemas?: SchemaDefinition[] }>(
      '/schemas',
      {},
      'Failed to fetch schemas'
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
        `Failed to fetch contents for ${contentType}`
      );
      const page = data.contents || [];
      items.push(...page);
      if (page.length < pageSize || items.length >= (data.total ?? 0)) return items;
    }
  },

  async getContent(contentType: string, slugOrId: string): Promise<ContentItem> {
    return request(`/contents/${contentType}/${slugOrId}`, {}, 'Content not found');
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
      'Failed to save content'
    );
  },

  async getAuditLogs(query: AuditLogQuery): Promise<AuditLogPage> {
    return request(`/audit-logs?${toQuery(query)}`, {}, 'Failed to fetch audit logs');
  },

  async listStorageObjects(query: StorageObjectQuery): Promise<StorageObjectPage> {
    return request(`/storage/objects?${toQuery(query)}`, {}, 'Failed to fetch media');
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
      'Failed to render the preview'
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
    const err = await res.json().catch(() => ({ error: 'Failed to delete the file' }));
    if (res.status === 409 && Array.isArray(err.references)) {
      return { deleted: false, references: err.references };
    }
    if (res.status === 401 && token) unauthorizedHandler?.();
    throw new ApiError(res.status, err.error || 'Failed to delete the file');
  },

  async uploadFile(file: File): Promise<StorageMeta> {
    const form = new FormData();
    form.append('file', file);
    return request('/storage/upload', { method: 'POST', body: form }, 'Failed to upload file');
  },

  async listThemes(): Promise<ThemeInfo[]> {
    return request('/themes', {}, 'Failed to fetch themes');
  },

  async getActiveTheme(): Promise<ThemeInfo> {
    return request('/themes/active', {}, 'Failed to fetch active theme');
  },

  async switchActiveTheme(themeId: string): Promise<ThemeInfo> {
    return request(
      '/themes/active',
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ theme_id: themeId }),
      },
      'Failed to switch active theme'
    );
  },
};
