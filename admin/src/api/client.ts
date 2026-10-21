import type {
  AuditLogPage,
  AuditLogQuery,
  ContentItem,
  CreateContentPayload,
  SchemaDefinition,
} from '../types';

const API_BASE = 'http://127.0.0.1:8080/api/v1';

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export const aegisApi = {
  async checkHealth(): Promise<{ status: string; system: string; version: string }> {
    const res = await fetch(`${API_BASE}/health`);
    if (!res.ok) throw new ApiError(res.status, 'Health check failed');
    return res.json();
  },

  async getSchemas(): Promise<SchemaDefinition[]> {
    const res = await fetch(`${API_BASE}/schemas`);
    if (!res.ok) throw new ApiError(res.status, 'Failed to fetch schemas');
    const data = await res.json();
    return data.schemas || [];
  },

  async getContents(contentType: string): Promise<ContentItem[]> {
    const res = await fetch(`${API_BASE}/contents/${contentType}`);
    if (!res.ok) throw new ApiError(res.status, `Failed to fetch contents for ${contentType}`);
    const data = await res.json();
    return data.contents || [];
  },

  async getContent(contentType: string, slugOrId: string): Promise<ContentItem> {
    const res = await fetch(`${API_BASE}/contents/${contentType}/${slugOrId}`);
    if (!res.ok) throw new ApiError(res.status, `Content not found`);
    return res.json();
  },

  async createContent(contentType: string, payload: CreateContentPayload): Promise<ContentItem> {
    const res = await fetch(`${API_BASE}/contents/${contentType}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to save content');
    }
    return res.json();
  },

  async getAuditLogs(sessionToken: string, query: AuditLogQuery): Promise<AuditLogPage> {
    const params = new URLSearchParams();
    for (const [key, value] of Object.entries(query)) {
      if (value !== undefined && value !== '') params.set(key, String(value));
    }
    const res = await fetch(`${API_BASE}/audit-logs?${params.toString()}`, {
      headers: { Authorization: `Bearer ${sessionToken}` },
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: 'Unknown server error' }));
      throw new ApiError(res.status, err.error || 'Failed to fetch audit logs');
    }
    return res.json();
  },
};
