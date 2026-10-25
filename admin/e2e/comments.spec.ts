import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { signIn } from './helpers';

// The server has [comments] enabled for articles (aegis.e2e.toml). Each test publishes its own
// article through the API and comments on its public page, as a reader would
const SERVER = 'http://127.0.0.1:18080';

const publishArticle = async (page: Page, slug: string) => {
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  const created = await page.request.post(`${SERVER}/api/v1/contents/article`, {
    headers: { Authorization: `Bearer ${token}` },
    data: { title: `Comments ${slug}`, slug, status: 'published', blocks: [] },
  });
  expect(created.ok()).toBeTruthy();
};

const postComment = async (page: Page, slug: string, name: string, body: string) => {
  await page.goto(`${SERVER}/articles/${slug}`);
  const form = page.locator('form.aegis-comment-form');
  await form.getByLabel('Name', { exact: true }).fill(name);
  await form.getByLabel('Email (not published)').fill(`${name.toLowerCase()}@example.com`);
  await form.getByLabel('Comment').fill(body);
  await form.getByRole('button', { name: 'Post comment' }).click();
};

const openComments = async (page: Page) => {
  await page
    .getByRole('navigation', { name: 'Views' })
    .getByRole('button', { name: 'Comments' })
    .click();
  await expect(page.getByRole('heading', { name: 'Comments', level: 1 })).toBeVisible();
};

test('a reader comments and an editor approves it from Admin Studio', async ({ page }) => {
  const slug = `e2e-approve-${Date.now()}`;
  await publishArticle(page, slug);

  await postComment(page, slug, 'Hanako', 'Waiting for approval');
  await expect(
    page.getByText('Thank you! Your comment will appear once it has been approved.')
  ).toBeVisible();
  await expect(page.getByRole('heading', { name: '0 comments' })).toBeVisible();

  await signIn(page);
  await openComments(page);
  const row = page.getByRole('row').filter({ hasText: 'Waiting for approval' });
  await expect(row.getByText('Pending', { exact: true })).toBeVisible();
  await row.getByRole('button', { name: 'Approve' }).click();
  await expect(page.getByText('Approved 1 comment')).toBeVisible();

  await page.goto(`${SERVER}/articles/${slug}`);
  await expect(page.getByRole('heading', { name: '1 comment' })).toBeVisible();
  await expect(page.locator('.aegis-comment')).toContainText('Waiting for approval');
});

test('approval can be switched off and back on from Admin Studio', async ({ page }) => {
  const slug = `e2e-open-${Date.now()}`;
  await publishArticle(page, slug);

  await signIn(page);
  await openComments(page);
  const approval = page.getByLabel('Hold new comments for approval before they are published');
  await expect(approval).toBeChecked();
  await approval.uncheck();
  await expect(page.getByText('Moderation settings saved')).toBeVisible();
  await expect(approval).not.toBeChecked();

  // Published right away: the page reloads at the new comment
  await postComment(page, slug, 'Taro', 'Published at once');
  await expect(page.getByRole('heading', { name: '1 comment' })).toBeVisible();
  await expect(page.locator('.aegis-comment')).toContainText('Published at once');

  // Back to the configuration file's value (approval on) for the other tests
  await page.goto('./');
  await openComments(page);
  await page.getByRole('button', { name: "Use the configuration file's values" }).click();
  await expect(
    page.getByLabel('Hold new comments for approval before they are published')
  ).toBeChecked();
});
