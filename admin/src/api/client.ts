import type {
  AuditLogPage,
  AuditLogQuery,
  ContentItem,
  CreateCategoryPayload,
  CreateContentPayload,
  SchemaDefinition,
  StorageMeta,
  StorageObjectPage,
  StorageObjectQuery,
} from '../types';
import { getSessionToken } from './session';

const API_BASE = 'http://127.0.0.1:8080/api/v1';

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

/** Headers carrying the current session token (the server requires one for every API but health) */
const authHeaders = (extra: Record<string, string> = {}): Record<string, string> => {
  const token = getSessionToken();
  return token ? { ...extra, Authorization: `Bearer ${token}` } : extra;
};

export const aegisApi = {
  async checkHealth(): Promise<{ status: string; system: string; version: string }> {
    const res = await fetch(`${API_BASE}/health`);
    if (!res.ok) throw new ApiError(res.status, 'Health check failed');
    return res.json();
  },

  async getSchemas(): Promise<SchemaDefinition[]> {
    const res = await fetch(`${API_BASE}/schemas`, { headers: authHeaders() });
    if (!res.ok) throw new ApiError(res.status, 'Failed to fetch schemas');
    const data = await res.json();
    return data.schemas || [];
  },

  async getContents(contentType: string): Promise<ContentItem[]> {
    const res = await fetch(`${API_BASE}/contents/${contentType}`, { headers: authHeaders() });
    if (!res.ok) throw new ApiError(res.status, `Failed to fetch contents for ${contentType}`);
    const data = await res.json();
    return data.contents || [];
  },

  async getContent(contentType: string, slugOrId: string): Promise<ContentItem> {
    const res = await fetch(`${API_BASE}/contents/${contentType}/${slugOrId}`, {
      headers: authHeaders(),
    });
    if (!res.ok) throw new ApiError(res.status, `Content not found`);
    return res.json();
  },

  async createCategory(payload: CreateCategoryPayload): Promise<ContentItem> {
    return this.createContent('category', payload);
  },

  async createContent(
    contentType: string,
    payload: CreateContentPayload | CreateCategoryPayload
  ): Promise<ContentItem> {
    const res = await fetch(`${API_BASE}/contents/${contentType}`, {
      method: 'POST',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to save content');
    }
    return res.json();
  },

  async getAuditLogs(query: AuditLogQuery): Promise<AuditLogPage> {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== '') params.set(key, String(value));
    }
    const res = await fetch(`${API_BASE}/audit-logs?${params.toString()}`, {
      headers: authHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to fetch audit logs');
    }
    return res.json();
  },

  async listStorageObjects(query: StorageObjectQuery): Promise<StorageObjectPage> {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== '') params.set(key, String(value));
    }
    const res = await fetch(`${API_BASE}/storage/objects?${params.toString()}`, {
      headers: authHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to fetch media');
    }
    return res.json();
  },

  async uploadFile(file: File): Promise<StorageMeta> {
    const form = new FormData();
    form.append('file', file);
    const res = await fetch(`${API_BASE}/storage/upload`, {
      method: 'POST',
      headers: authHeaders(),
      body: form,
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to upload file');
    }
    return res.json();
  },
};
