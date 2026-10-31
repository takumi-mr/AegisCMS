import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchPluginInfo, fetchFixtures, invokeHook, pollVersion } from './api.ts';

describe('plugin-builder api', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('fetchPluginInfo parses successful response', async () => {
    const mockInfo = {
      manifest: { name: 'test-plugin', version: '1.0.0' },
      entrypoint: 'plugin.wasm',
      wasm_exists: true,
      wasm_size_bytes: 1024,
      exports: ['on_before_save'],
      updated_at: 100,
    };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockInfo,
    });
    vi.stubGlobal('fetch', mockFetch);

    const info = await fetchPluginInfo();
    expect(info.manifest.name).toBe('test-plugin');
    expect(info.exports).toContain('on_before_save');
    expect(mockFetch).toHaveBeenCalledWith('/__dev/api/plugin');
  });

  it('fetchFixtures retrieves fixture presets', async () => {
    const mockFixtures = {
      'on-before-save': { title: 'Hello' },
    };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockFixtures,
    });
    vi.stubGlobal('fetch', mockFetch);

    const fixtures = await fetchFixtures();
    expect(fixtures['on-before-save']).toEqual({ title: 'Hello' });
    expect(mockFetch).toHaveBeenCalledWith('/__dev/api/fixtures');
  });

  it('invokeHook sends WRITE_HEADER and json payload', async () => {
    const mockResponse = {
      success: true,
      output: { title: 'Transformed' },
      fuel_consumed: 1200,
      duration_ms: 1.5,
      logs: [],
      kv_mutations: {},
      http_requests: [],
    };
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => mockResponse,
    });
    vi.stubGlobal('fetch', mockFetch);

    const res = await invokeHook({
      hook: 'on-before-save',
      input: { title: 'Input' },
    });

    expect(res.success).toBe(true);
    expect(res.fuel_consumed).toBe(1200);
    expect(mockFetch).toHaveBeenCalledWith('/__dev/api/invoke', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'x-aegis-dev': '1',
      },
      body: JSON.stringify({ hook: 'on-before-save', input: { title: 'Input' } }),
    });
  });

  it('pollVersion calls with since query param', async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ version: 12345 }),
    });
    vi.stubGlobal('fetch', mockFetch);

    const v = await pollVersion(100);
    expect(v).toBe(12345);
    expect(mockFetch).toHaveBeenCalledWith('/__dev/version?since=100');
  });
});
