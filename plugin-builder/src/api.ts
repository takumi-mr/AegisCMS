import type { PluginInfo, InvokeRequest, InvokeResponse } from './types.ts';

const WRITE_HEADER = 'x-aegis-dev';

export async function fetchPluginInfo(): Promise<PluginInfo> {
  const res = await fetch('/__dev/api/plugin');
  if (!res.ok) {
    throw new Error(`Failed to fetch plugin info: ${res.statusText}`);
  }
  return res.json();
}

export async function fetchFixtures(): Promise<Record<string, unknown>> {
  const res = await fetch('/__dev/api/fixtures');
  if (!res.ok) {
    throw new Error(`Failed to fetch fixtures: ${res.statusText}`);
  }
  return res.json();
}

export async function invokeHook(req: InvokeRequest): Promise<InvokeResponse> {
  const res = await fetch('/__dev/api/invoke', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      [WRITE_HEADER]: '1',
    },
    body: JSON.stringify(req),
  });
  if (!res.ok) {
    const errorText = await res.text();
    throw new Error(`Failed to invoke hook: ${res.status} - ${errorText}`);
  }
  return res.json();
}

export async function pollVersion(since?: number): Promise<number> {
  const url = since !== undefined ? `/__dev/version?since=${since}` : '/__dev/version';
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to poll version: ${res.status}`);
  }
  const data = await res.json();
  return data.version;
}
