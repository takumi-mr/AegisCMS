import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { aegisApi } from './client';

// The content list calls of the client: what is asked of the server, and what it gives back
const fetchMock = vi.fn();

const respond = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

const urls = () => fetchMock.mock.calls.map(([url]) => String(url));

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('content summaries', () => {
  it('asks for the summary view with only the search words and filters given', async () => {
    fetchMock.mockImplementation(async () =>
      respond(200, {
        contents: [{ id: 'a1', title: 'A', slug: 'a', excerpt: 'About A' }],
        total: 41,
        limit: 20,
        offset: 20,
      })
    );
    const page = await aegisApi.listContentSummaries('article', {
      q: 'rust guide',
      category: 'none',
      status: '',
      limit: 20,
      offset: 20,
    });
    expect(page).toMatchObject({ total: 41, limit: 20, offset: 20 });
    expect(page.items.map((item) => item.id)).toEqual(['a1']);

    const query = new URL(urls()[0], 'http://localhost').searchParams;
    expect(Object.fromEntries(query)).toEqual({
      q: 'rust guide',
      category: 'none',
      limit: '20',
      offset: '20',
      view: 'summary',
    });
    expect(urls()[0]).toMatch(/\/api\/v1\/contents\/article\?/);
  });

  it('never fetches the blocks or the rendered HTML of every item', async () => {
    fetchMock.mockImplementation(async () => respond(200, { contents: [], total: 0 }));
    await aegisApi.listContentSummaries('page');
    await aegisApi.getAllContentSummaries('category');
    for (const url of urls()) {
      expect(new URL(url, 'http://localhost').searchParams.get('view')).toBe('summary');
    }
  });

  it('reads every page of a small list (the categories)', async () => {
    const item = (n: number) => ({ id: `c${n}`, title: `C${n}`, slug: `c${n}` });
    fetchMock.mockImplementation(async (url: string) => {
      const offset = Number(new URL(url, 'http://localhost').searchParams.get('offset'));
      const all = Array.from({ length: 501 }, (_, n) => item(n));
      return respond(200, { contents: all.slice(offset, offset + 500), total: 501 });
    });
    const items = await aegisApi.getAllContentSummaries('category');
    expect(items).toHaveLength(501);
    expect(
      urls().map((url) => new URL(url, 'http://localhost').searchParams.get('offset'))
    ).toEqual(['0', '500']);
  });
});
