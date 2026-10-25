import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, aegisApi } from './client';

// The notification API calls of the client: paths, methods and bodies, and how errors come back
const fetchMock = vi.fn();

const respond = (status: number, body?: unknown) =>
  new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

const lastCall = () => {
  const [url, init] = fetchMock.mock.calls.at(-1) as [string, RequestInit | undefined];
  return { url, method: init?.method ?? 'GET', body: init?.body as string | undefined };
};

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal('fetch', fetchMock);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('notification queue', () => {
  it('lists with only the filters given', async () => {
    fetchMock.mockImplementation(async () => respond(200, { items: [], total: 0 }));
    await aegisApi.listNotificationQueue({ status: 'failed', limit: 50 });
    expect(lastCall().url).toMatch(/\/api\/v1\/notifications\/queue\?status=failed&limit=50$/);
    await aegisApi.listNotificationQueue({});
    expect(lastCall().url).toMatch(/\/notifications\/queue\?$/);
  });

  it('retries and discards by id (204 No Content)', async () => {
    fetchMock.mockImplementation(async () => respond(204));
    await aegisApi.retryNotification('a/b');
    expect(lastCall()).toMatchObject({ method: 'POST' });
    expect(lastCall().url).toMatch(/\/notifications\/queue\/a%2Fb\/retry$/);
    await aegisApi.discardNotification('n1');
    expect(lastCall()).toMatchObject({ method: 'DELETE' });
    expect(lastCall().url).toMatch(/\/notifications\/queue\/n1$/);
  });

  it('turns an error response into an ApiError with the server message', async () => {
    fetchMock.mockResolvedValue(
      respond(409, { error: 'Only notifications that failed can be sent again' })
    );
    const error = await aegisApi.retryNotification('n1').catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 409,
      message: 'Only notifications that failed can be sent again',
    });
  });
});

describe('notification templates', () => {
  it('saves and resets a template by its name', async () => {
    const detail = { name: 'test.subject', current: null, default: null };
    fetchMock.mockImplementation(async () => respond(200, detail));
    await aegisApi.saveNotificationTemplate('test.subject', 'Hi {{ site_name }}');
    expect(lastCall()).toMatchObject({
      method: 'PUT',
      body: JSON.stringify({ content: 'Hi {{ site_name }}' }),
    });
    expect(lastCall().url).toMatch(/\/notifications\/templates\/test\.subject$/);
    expect(await aegisApi.resetNotificationTemplate('test.subject')).toEqual(detail);
    expect(lastCall().method).toBe('DELETE');
  });

  it('previews with the unsaved templates', async () => {
    fetchMock.mockImplementation(async () => respond(200, { subject: 'S', body: 'B', html: null }));
    const preview = await aegisApi.previewNotification({
      kind: 'order.placed',
      staff: true,
      templates: { 'order.placed.staff.subject': 'New' },
    });
    expect(preview.subject).toBe('S');
    expect(lastCall().method).toBe('POST');
    expect(JSON.parse(lastCall().body ?? '')).toEqual({
      kind: 'order.placed',
      staff: true,
      templates: { 'order.placed.staff.subject': 'New' },
    });
  });
});

describe('test send', () => {
  it('posts the kind and recipients and returns each delivery', async () => {
    const deliveries = [
      { channel: 'email', recipient: 'you@example.com', subject: 'S', ok: true, error: null },
    ];
    fetchMock.mockImplementation(async () => respond(200, deliveries));
    expect(
      await aegisApi.sendTestNotification({ kind: 'test', to: 'you@example.com', staff: false })
    ).toEqual(deliveries);
    expect(lastCall().url).toMatch(/\/api\/v1\/notifications\/test$/);
    expect(JSON.parse(lastCall().body ?? '')).toEqual({
      kind: 'test',
      to: 'you@example.com',
      staff: false,
    });
  });
});
