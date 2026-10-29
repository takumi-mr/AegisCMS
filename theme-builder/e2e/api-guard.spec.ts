import { expect, test } from '@playwright/test';

import { DEV_SERVER_PORT, resetTheme } from './fixture.mjs';
import { readTheme } from './helpers.ts';

const api = `http://127.0.0.1:${DEV_SERVER_PORT}/__dev/api`;

test.beforeEach(() => resetTheme());

// What a page on another site could do against the dev server on the developer's machine
test('refuses writes without the x-aegis-dev header', async ({ request }) => {
  const before = readTheme('theme.json');
  const res = await request.put(`${api}/files/theme.json`, { data: { content: '{}' } });
  expect(res.status()).toBe(403);
  expect(readTheme('theme.json')).toBe(before);
});

test('accepts the same write with the header', async ({ request }) => {
  const res = await request.put(`${api}/files/assets/style.css`, {
    headers: { 'x-aegis-dev': '1' },
    data: { content: 'body{}' },
  });
  expect(res.status()).toBe(204);
  expect(readTheme('assets/style.css')).toBe('body{}');
});

test('refuses paths outside the theme files', async ({ request }) => {
  // `..` written as a path segment is resolved by the HTTP client before it is sent (and
  // `templates/../theme.json` is then the allowed `/theme.json`), so the slashes are encoded here
  for (const path of [
    '..%2Fsecret.html',
    'templates%2F..%2F..%2Fsecret.html',
    'templates%2F..%2Ftheme.json',
    'config/default.toml',
  ]) {
    const res = await request.put(`${api}/files/${path}`, {
      headers: { 'x-aegis-dev': '1' },
      data: { content: 'x' },
    });
    expect([400, 404]).toContain(res.status());
  }
});

test('refuses a Host that is not a loopback name (DNS rebinding)', async ({ request }) => {
  const res = await request.get(`${api}/files`, { headers: { host: 'evil.example' } });
  expect(res.status()).toBe(403);
});
