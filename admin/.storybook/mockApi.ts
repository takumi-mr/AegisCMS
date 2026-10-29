// Answers Admin Studio's API requests (`aegisApi` in src/api/client.ts) inside stories, so a
// component that loads its own data renders without a core server.

export interface MockRequest {
  method: string;
  /** Path after `/api/v1`, without the query string (e.g. `/plugins/seo/enabled`) */
  path: string;
  body: unknown;
}

/** A JSON body (status 200), or `{ status, body }` for other statuses */
export type MockResult = unknown | { status: number; body: unknown };

/** Returns the response, or `undefined` to fall through to a 404 */
export type MockHandler = (request: MockRequest) => MockResult | undefined | Promise<MockResult>;

const isWithStatus = (r: unknown): r is { status: number; body: unknown } =>
  typeof r === 'object' && r !== null && 'status' in r && 'body' in r;

/**
 * Replaces `window.fetch` for `/api/v1/*` until the returned cleanup runs (return it from a
 * story's `beforeEach`). Other URLs (Vite modules, fonts) go to the real fetch.
 */
export const mockApi = (handler: MockHandler, { delayMs = 150 } = {}): (() => void) => {
  const realFetch = window.fetch;
  window.fetch = async (input, init) => {
    const url = new URL(input instanceof Request ? input.url : String(input), window.location.href);
    const marker = url.pathname.indexOf('/api/v1/');
    if (marker === -1) return realFetch(input, init);

    const text = typeof init?.body === 'string' ? init.body : undefined;
    const result = await handler({
      method: (init?.method ?? 'GET').toUpperCase(),
      path: url.pathname.slice(marker + '/api/v1'.length),
      body: text ? JSON.parse(text) : undefined,
    });
    await new Promise((resolve) => setTimeout(resolve, delayMs));

    const { status, body } =
      result === undefined
        ? { status: 404, body: { error: 'Not mocked in this story' } }
        : isWithStatus(result)
          ? result
          : { status: 200, body: result };
    // A 204 response must not have a body (the Response constructor throws)
    return new Response(status === 204 ? null : JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    });
  };
  return () => {
    window.fetch = realFetch;
  };
};
