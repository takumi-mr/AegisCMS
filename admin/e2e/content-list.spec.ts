import { expect, test, type Page } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { signIn } from './helpers';

// The list in the sidebar is searched, filtered and paged by the server (20 items per page), and
// holds summaries only: the whole item is fetched when one is opened
const SERVER = 'http://127.0.0.1:18080';
const PAGE_SIZE = 20;

/** Creates drafts through the API; the last one has a summary (`excerpt`) with a unique word */
const createArticles = async (page: Page, prefix: string, count: number, summaryWord: string) => {
  const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  const { id: token } = await login.json();
  for (let n = 1; n <= count; n++) {
    const last = n === count;
    const created = await page.request.post(`${SERVER}/api/v1/contents/article`, {
      headers: { Authorization: `Bearer ${token}` },
      data: {
        title: `${prefix} ${String(n).padStart(2, '0')}`,
        slug: `${prefix.toLowerCase().replace(/\s+/g, '-')}-${n}`,
        status: 'draft',
        blocks: [{ name: 'core/paragraph', attributes: {}, text: `Body of item ${n}` }],
        ...(last ? { excerpt: `A summary about ${summaryWord}` } : {}),
      },
    });
    expect(created.ok()).toBeTruthy();
  }
};

test.describe('Content list', () => {
  test('searches, filters and pages the list on the server', async ({ page }) => {
    const suffix = Date.now();
    const prefix = `Listing ${suffix}`;
    const summaryWord = `zebra${suffix}`;
    // One more than a page, so the second page has exactly one item
    await createArticles(page, prefix, PAGE_SIZE + 1, summaryWord);
    await signIn(page);

    const sidebar = page.locator('aside');
    const cards = sidebar.getByRole('button').filter({ hasText: prefix });
    const search = sidebar.getByRole('searchbox', { name: 'Search content' });
    const pager = sidebar.getByRole('navigation', { name: 'Pages of the list' });

    // Search by title: the count is the number of matches, and only a page of them is shown
    await search.fill(prefix);
    await expect(sidebar.getByText(`Articles (${PAGE_SIZE + 1})`)).toBeVisible();
    await expect(cards).toHaveCount(PAGE_SIZE);
    await expect(pager.getByText(`1–${PAGE_SIZE} of ${PAGE_SIZE + 1}`)).toBeVisible();
    await expect(pager.getByRole('button', { name: 'Previous page' })).toBeDisabled();

    // The next page has the rest, the previous page brings the first ones back
    await pager.getByRole('button', { name: 'Next page' }).click();
    await expect(
      pager.getByText(`${PAGE_SIZE + 1}–${PAGE_SIZE + 1} of ${PAGE_SIZE + 1}`)
    ).toBeVisible();
    await expect(cards).toHaveCount(1);
    await expect(pager.getByRole('button', { name: 'Next page' })).toBeDisabled();
    await pager.getByRole('button', { name: 'Previous page' }).click();
    await expect(cards).toHaveCount(PAGE_SIZE);

    // A new search starts from the first page; the summary is searched too and is shown on the card
    await search.fill(summaryWord);
    await expect(sidebar.getByText('Articles (1)')).toBeVisible();
    await expect(cards).toHaveCount(1);
    await expect(cards.first()).toContainText(`A summary about ${summaryWord}`);
    await expect(pager).toHaveCount(0);

    // Filters narrow the search: all of these are drafts without a category
    await sidebar.getByRole('combobox', { name: 'Filter by status' }).selectOption('published');
    await expect(sidebar.getByText('Nothing matches your search or filters.')).toBeVisible();
    await sidebar.getByRole('combobox', { name: 'Filter by status' }).selectOption('draft');
    await expect(cards).toHaveCount(1);
    await sidebar
      .getByRole('combobox', { name: 'Filter by category' })
      .selectOption({ label: 'Uncategorized' });
    await expect(cards).toHaveCount(1);

    // Opening an item fetches the whole item: the title and the body come with it
    await cards.first().click();
    await expect(page.getByPlaceholder('Enter post title...')).toHaveValue(
      `${prefix} ${PAGE_SIZE + 1}`
    );
    await expect(page.getByRole('textbox', { name: /Start typing your paragraph/i })).toHaveValue(
      `Body of item ${PAGE_SIZE + 1}`
    );

    // Clearing the search shows the list again
    await sidebar.getByRole('button', { name: 'Clear search' }).click();
    await sidebar.getByRole('combobox', { name: 'Filter by status' }).selectOption('');
    await sidebar.getByRole('combobox', { name: 'Filter by category' }).selectOption('');
    await expect(cards).toHaveCount(PAGE_SIZE);
  });

  test('the list does not fetch the blocks or rendered HTML of every item', async ({ page }) => {
    await signIn(page);
    const listRequests: string[] = [];
    page.on('request', (request) => {
      const url = new URL(request.url());
      if (/^\/api\/v1\/contents\/(article|page|category)$/.test(url.pathname)) {
        listRequests.push(url.search);
      }
    });
    // Another type is another list
    await page.locator('aside').getByRole('tab', { name: 'Pages' }).click();
    await expect.poll(() => listRequests.length).toBeGreaterThan(0);
    for (const search of listRequests) {
      expect(new URLSearchParams(search).get('view')).toBe('summary');
    }
  });
});
