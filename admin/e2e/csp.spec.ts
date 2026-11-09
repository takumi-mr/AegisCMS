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

  expect(scriptViolations(seen)).toEqual([]);
  expect(errors).toEqual([]);
  // Style / font / image violations are worth knowing about, but are not this test's concern
  if (seen.length > 0) {
    test.info().annotations.push({
      type: 'other CSP violations',
      description: JSON.stringify(seen.slice(0, 10)),
    });
  }
});
