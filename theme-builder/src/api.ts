import type { CheckResult, FileList, RenderRequest, RenderResult, ThemeInfo } from './types.ts';

/** Sent on every write: a page on another site cannot add it without a CORS preflight */
export const WRITE_HEADER = 'x-aegis-dev';

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function request<T>(
  path: string,
  init: { method?: string; body?: unknown; signal?: AbortSignal } = {}
): Promise<T> {
  const method = init.method ?? 'GET';
  const headers: Record<string, string> = {};
  if (method !== 'GET') headers[WRITE_HEADER] = '1';
  if (init.body !== undefined) headers['content-type'] = 'application/json';
  const res = await fetch(path, {
    method,
    headers,
    body: init.body === undefined ? undefined : JSON.stringify(init.body),
    signal: init.signal,
  });
  if (!res.ok) {
    let message = `${res.status} ${res.statusText}`;
    try {
      const data = await res.json();
      if (typeof data?.error === 'string') message = data.error;
    } catch {
      // not JSON: keep the status line
    }
    throw new ApiError(res.status, message);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Path segments are encoded one by one, so `/` stays a separator */
export function filePath(path: string): string {
  return `/__dev/api/files/${path.split('/').map(encodeURIComponent).join('/')}`;
}

export const api = {
  theme: () => request<ThemeInfo>('/__dev/api/theme'),
  files: () => request<FileList>('/__dev/api/files'),
  readFile: (path: string) => request<{ path: string; content: string }>(filePath(path)),
  writeFile: (path: string, content: string) =>
    request<void>(filePath(path), { method: 'PUT', body: { content } }),
  deleteFile: (path: string) => request<void>(filePath(path), { method: 'DELETE' }),
  renameFile: (from: string, to: string) =>
    request<void>('/__dev/api/rename', { method: 'POST', body: { from, to } }),
  render: (body: RenderRequest, signal?: AbortSignal) =>
    request<RenderResult>('/__dev/api/render', { method: 'POST', body, signal }),
  check: () => request<CheckResult>('/__dev/api/check'),
  /** Resolves with the version once it differs from `since` (long poll, ~25s) */
  version: (since?: number, signal?: AbortSignal) =>
    request<{ version: number }>(
      since === undefined ? '/__dev/version' : `/__dev/version?since=${since}`,
      { signal }
    ),
};
