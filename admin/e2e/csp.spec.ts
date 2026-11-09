import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { openView, signIn } from './helpers';

// The default Content-Security-Policy has no 'unsafe-eval' (and Admin Studio needs no inline script).
// These tests walk through Admin Studio and the public pages and fail on any script the policy
// blocks (`script-src`: eval, an inline script, a script from another host) or any script error,
// so a feature that starts to need `eval` is caught here.
const SERVER = 'http://127.0.0.1:18080';

interface Violation {
  directive: string;
  blocked: string;
  sample: string;
}

/** Records every CSP violation of every document the page loads */
const recordViolations = async (page: Page) => {
  await page.addInitScript(() => {
    const w = window as unknown as { __csp: Violation[] };
    w.__csp = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      w.__csp.push({
        directive: e.effectiveDirective,
        blocked: e.blockedURI,
        sample: e.sample,
      });
    });
  });
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  return errors;
};

const violations = (page: Page) =>
  page.evaluate(() => (window as unknown as { __csp?: Violation[] }).__csp ?? []);

/** Script violations fail the test; other directives (style, font, image) are only reported */
const scriptViolations = (all: Violation[]) =>
  all.filter((v) => v.directive.startsWith('script-src'));

test('Admin Studio runs under the default CSP without eval or inline scripts', async ({ page }) => {
  const errors = await recordViolations(page);
  await signIn(page);

  const seen: Violation[] = [];
  const collect = async () => seen.push(...(await violations(page)));

  for (const view of ['Calendar', 'Products', 'Orders', 'Members', 'Settings', 'Editor'] as const) {
    await openView(page, view);
    await page.waitForLoadState('networkidle');
    await collect();
  }

  // Every Settings section (themes, users, plugins, headers, ...)
  await openView(page, 'Settings');
  const nav = page.getByRole('navigation', { name: 'Settings sections' });
  const names = await nav.getByRole('button').allInnerTexts();
  expect(names.length).toBeGreaterThan(3);
  for (const name of names) {
    await nav.getByRole('button', { name: name.trim(), exact: true }).click();
    await page.waitForLoadState('networkidle');
    await collect();
  }

  expect(scriptViolations(seen)).toEqual([]);
  expect(errors).toEqual([]);
});

test('public pages run under the default CSP without eval or blocked scripts', async ({ page }) => {
  const errors = await recordViolations(page);

  // A published article (the comment form has an inline script)
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const slug = `csp-${Date.now()}`;
  const created = await page.request.post(`${SERVER}/api/v1/contents/article`, {
    headers: { Authorization: `Bearer ${token}` },
    data: {
      title: `CSP ${slug}`,
      slug,
      status: 'published',
      blocks: [{ name: 'core/paragraph', attributes: {}, text: 'Body text' }],
    },
  });
  expect(created.ok()).toBeTruthy();

  const seen: Violation[] = [];
  for (const path of [
    '/',
    '/articles',
    `/articles/${slug}`,
    '/search?q=body',
    '/account/login',
    '/contact',
  ]) {
    const response = await page.goto(`${SERVER}${path}`);
    expect(response?.status(), path).toBeLessThan(500);
    await page.waitForLoadState('networkidle');
    seen.push(...(await violations(page)));
  }
  // The comment form is the page's inline script: it ran (it adds the saved-author handling)
  await page.goto(`${SERVER}/articles/${slug}`);
  await expect(page.locator('form.cms-comment-form')).toBeVisible();

  // Nothing the policy blocks: not a script, and (since the default theme serves its own fonts)
  // not a style sheet, a font or an image either
  expect(seen).toEqual([]);
  expect(errors).toEqual([]);

  // The default theme's fonts come from this site, and they load
  const fontRequests: string[] = [];
  page.on(
    'response',
    (r) => r.url().includes('/fonts/') && fontRequests.push(`${r.status()} ${r.url()}`)
  );
  await page.goto(`${SERVER}/`);
  const loaded = await page.evaluate(
    async () => (await document.fonts.load('16px "Plus Jakarta Sans"', 'A')).length
  );
  expect(loaded).toBeGreaterThan(0);
  expect(fontRequests.some((r) => r.startsWith('200 ') && r.includes('plus-jakarta-sans'))).toBe(
    true
  );
  expect(fontRequests.every((r) => r.startsWith('200 ') || r.startsWith('304 '))).toBe(true);
});

test('the themed-page preview runs under the public CSP, isolated from Admin Studio', async ({
  page,
}) => {
  await signIn(page);
  await page.getByRole('button', { name: 'New' }).click();
  await page.getByPlaceholder('Enter post title...').fill('Preview frame');
  await page.getByRole('tab', { name: 'Themed Page' }).click();
  const frame = page.frameLocator('iframe[title="Themed page preview"]');
  await expect(frame.locator('article, main').first()).toBeVisible({ timeout: 15_000 });

  // The preview frame follows the public site's CSP, not Admin Studio's (a `srcdoc` frame would
  // inherit Admin Studio's). The E2E server runs the public site under the strict CSP, so a script
  // nobody hashed (this probe) is blocked, as on a public page; the host-hashed ones run (below)
  const frameHandle = await page.locator('iframe[title="Themed page preview"]').elementHandle();
  const inner = await frameHandle!.contentFrame();
  const ran = await inner!.evaluate(() => {
    const script = document.createElement('script');
    script.textContent = 'window.__previewScript = 1';
    document.head.appendChild(script);
    return (window as unknown as { __previewScript?: number }).__previewScript === 1;
  });
  expect(ran).toBe(false);

  // ...and it cannot reach the admin's session (an opaque origin: `sandbox` without same-origin)
  const storage = await inner!.evaluate(() => {
    try {
      return sessionStorage.length >= 0 ? 'readable' : 'empty';
    } catch {
      return 'blocked';
    }
  });
  expect(storage).toBe('blocked');

  // The "open in a separate window" button opens the same page, just as isolated
  const [popup] = await Promise.all([
    page.waitForEvent('popup'),
    page.getByRole('button', { name: 'Open in a separate window' }).click(),
  ]);
  await popup.waitForLoadState('domcontentloaded');
  expect(popup.url()).toContain('/api/v1/preview/frame/');
  await expect(popup.locator('article, main').first()).toBeVisible();
  const popupStorage = await popup.evaluate(() => {
    try {
      return sessionStorage.length >= 0 ? 'readable' : 'empty';
    } catch {
      return 'blocked';
    }
  });
  expect(popupStorage).toBe('blocked');
});

test("the built-in comment form works without any inline script (script-src 'self' only)", async ({
  page,
}) => {
  const errors = await recordViolations(page);
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const auth = { Authorization: `Bearer ${token}` };
  const slug = `csp-strict-${Date.now()}`;
  const created = await page.request.post(`${SERVER}/api/v1/contents/article`, {
    headers: auth,
    data: { title: `Strict ${slug}`, slug, status: 'published', blocks: [] },
  });
  expect(created.ok()).toBeTruthy();

  // The strictest policy the site can ask for: scripts only from its own origin
  const strict =
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'self'; form-action 'self';";
  const set = await page.request.put(`${SERVER}/api/v1/headers`, {
    headers: auth,
    data: { content_security_policy: strict },
  });
  expect(set.ok()).toBeTruthy();
  try {
    await page.goto(`${SERVER}/articles/${slug}`);
    const form = page.locator('form.cms-comment-form');
    await form.getByLabel('Name', { exact: true }).fill('Strict');
    await form.getByLabel('Email (not published)').fill('strict@example.com');
    await form.getByLabel('Comment', { exact: true }).fill('Posted under a strict CSP');
    await form.getByRole('button', { name: 'Post comment' }).click();
    // The form script ran (it is an external file now): it sends the comment and shows the result
    await expect(
      page.getByText('Thank you! Your comment will appear once it has been approved.')
    ).toBeVisible();
    expect(scriptViolations(await violations(page))).toEqual([]);
    expect(errors).toEqual([]);

    // The setup page's script is external too (the page itself is for a new site, so only the file)
    const setupJs = await page.request.get(`${SERVER}/_cms/setup.js`);
    expect(setupJs.ok()).toBeTruthy();
  } finally {
    // Back to the defaults
    await page.request.put(`${SERVER}/api/v1/headers`, { headers: auth, data: {} });
  }
});

test("under the strict CSP, a plugin's inline script runs because the host hashed it", async ({
  page,
}) => {
  const errors = await recordViolations(page);
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const auth = { Authorization: `Bearer ${token}` };
  const slug = `csp-plugin-${Date.now()}`;
  const created = await page.request.post(`${SERVER}/api/v1/contents/article`, {
    headers: auth,
    data: {
      title: `Plugin ${slug}`,
      slug,
      status: 'published',
      blocks: [
        {
          name: 'core/paragraph',
          attributes: {},
          text: 'See [an external page](https://example.org/).',
        },
      ],
    },
  });
  expect(created.ok()).toBeTruthy();

  const enable = (enabled: boolean) =>
    page.request.put(`${SERVER}/api/v1/plugins/official-external-links/enabled`, {
      headers: auth,
      data: { enabled },
    });
  expect((await enable(true)).ok()).toBeTruthy();
  try {
    const response = await page.goto(`${SERVER}/articles/${slug}`);
    const csp = response!.headers()['content-security-policy'];
    const scriptSrc = csp.split(';').find((d) => d.trim().startsWith('script-src')) ?? '';
    // No `unsafe-inline`: the plugin's inline script is allowed by its hash only
    expect(scriptSrc).not.toContain('unsafe-inline');
    expect(scriptSrc).toContain("'sha256-");
    // The script ran: it marks the external link (it adds this class)
    await expect(page.locator('a[href="https://example.org/"].oel-external')).toBeVisible();
    expect(scriptViolations(await violations(page))).toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    await enable(false);
  }
});

test('a plugin script in the themed-page preview runs by its hash, as on the public site', async ({
  page,
}) => {
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const auth = { Authorization: `Bearer ${token}` };
  const enable = (enabled: boolean) =>
    page.request.put(`${SERVER}/api/v1/plugins/official-external-links/enabled`, {
      headers: auth,
      data: { enabled },
    });
  expect((await enable(true)).ok()).toBeTruthy();
  try {
    const key = 'abcdef0123456789abcdef0123456789';
    const preview = await page.request.post(`${SERVER}/api/v1/preview`, {
      headers: auth,
      data: {
        content_type: 'article',
        title: 'Hashed preview',
        slug: 'hashed-preview',
        fields: {},
        frame_key: key,
        blocks: [
          { name: 'core/paragraph', attributes: {}, text: 'See [a page](https://example.org/).' },
        ],
      },
    });
    expect(preview.ok()).toBeTruthy();
    const { page_url: pageUrl } = await preview.json();
    expect(pageUrl).toContain(`/preview/frame/${key}`);

    const response = await page.goto(`${SERVER}/api/v1${pageUrl}`);
    const csp = response!.headers()['content-security-policy'];
    // The public site's policy plus the sandbox; the hash was stored with the page
    expect(csp).toContain('sandbox allow-scripts');
    expect(csp).toContain("'sha256-");
    expect(csp).not.toMatch(/script-src[^;]*unsafe-inline/);
    await expect(page.locator('a[href="https://example.org/"].oel-external')).toBeVisible();
  } finally {
    await enable(false);
  }
});

test("Admin Studio needs no inline style: its screens and previews run under style-src 'self'", async ({
  page,
}) => {
  await recordViolations(page);
  await signIn(page);
  const seen: Violation[] = [];
  const collect = async () => seen.push(...(await violations(page)));

  // The editor with the three preview tabs
  await page.getByRole('button', { name: 'New' }).click();
  await page.getByPlaceholder('Enter post title...').fill('Style check');
  for (const tab of ['Live HTML Output', 'Themed Page', 'Block AST (JSON)']) {
    await page.getByRole('tab', { name: tab }).click();
    await page.waitForTimeout(800);
    await collect();
  }
  // Settings (notifications has the HTML mail preview), plugins, themes
  const nav = page.getByRole('navigation', { name: 'Settings sections' });
  await openView(page, 'Settings');
  for (const name of await nav.getByRole('button').allInnerTexts()) {
    await nav.getByRole('button', { name: name.trim(), exact: true }).click();
    await page.waitForLoadState('networkidle');
    await collect();
  }
  // Nothing of Admin Studio's own page was blocked (the frames are separate documents)
  expect(seen.filter((v) => v.directive.startsWith('style-src'))).toEqual([]);
});

test("the body HTML preview keeps a plugin block's own <style>, in a sandboxed frame", async ({
  page,
}) => {
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const auth = { Authorization: `Bearer ${token}` };
  const enable = (enabled: boolean) =>
    page.request.put(`${SERVER}/api/v1/plugins/official-blocks/enabled`, {
      headers: auth,
      data: { enabled },
    });
  expect((await enable(true)).ok()).toBeTruthy();
  try {
    const key = '0123456789abcdef0123456789abcdef';
    const preview = await page.request.post(`${SERVER}/api/v1/preview`, {
      headers: auth,
      data: {
        content_type: 'article',
        title: 'Body preview',
        slug: 'body-preview',
        fields: {},
        body_frame_key: key,
        blocks: [
          { name: 'core/paragraph', attributes: {}, text: 'Body text' },
          {
            name: 'official-blocks/button',
            attributes: { label: 'Go', url: 'https://example.org/', style: 'primary' },
          },
        ],
      },
    });
    expect(preview.ok()).toBeTruthy();
    const { body_url: bodyUrl } = await preview.json();
    expect(bodyUrl).toContain(`/preview/frame/${key}`);

    // The block's <style> works there (the public site's CSP allows it), and it is isolated
    const response = await page.goto(`${SERVER}/api/v1${bodyUrl}`);
    expect(response!.headers()['content-security-policy']).toContain('sandbox allow-scripts');
    await expect(page.getByText('Body text')).toBeVisible();
    const display = await page
      .locator('a.ob-button')
      .evaluate((el) => getComputedStyle(el).display);
    expect(display).toBe('inline-block');
  } finally {
    await enable(false);
  }
});
