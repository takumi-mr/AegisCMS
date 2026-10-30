import { expect, test } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { openView, signIn } from './helpers';

const SERVER = 'http://127.0.0.1:18080';
const PAGE_SIZE = 25;

test.describe('Commerce (Products & Orders)', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates a product and verifies it in product catalog', async ({ page }) => {
    // Open Products view
    await openView(page, 'Products');
    await expect(page.getByRole('heading', { name: 'Products', level: 1 })).toBeVisible();

    const productTitle = `Mug ${Date.now()}`;
    const productPrice = '1800';
    const productStock = '50';

    // Click "New product"
    await page.getByRole('button', { name: 'New product' }).click();

    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();

    // Fill Product form
    await modal.getByLabel('Title').fill(productTitle);
    await modal.getByLabel('Price', { exact: true }).fill(productPrice);
    await modal.getByLabel('Stock').fill(productStock);

    // Save product
    await modal.getByRole('button', { name: 'Create product' }).click();

    // Verify creation toast and listing in catalog
    await expect(page.getByText(`Created "${productTitle}"`)).toBeVisible();
    await expect(page.getByText(productTitle, { exact: true })).toBeVisible();
  });

  test('searches, filters and pages the product list on the server', async ({ page }) => {
    const suffix = Date.now();
    const prefix = `Bulk ${suffix}`;
    const sku = `SKU-${suffix}`;
    const login = await page.request.post(`${SERVER}/api/v1/auth/session`, {
      data: { username: E2E_USERNAME, password: E2E_PASSWORD },
    });
    expect(login.ok()).toBeTruthy();
    const { id: token } = await login.json();
    // One more than a page; the last one has a SKU to search for
    for (let n = 1; n <= PAGE_SIZE + 1; n++) {
      const created = await page.request.post(`${SERVER}/api/v1/contents/product`, {
        headers: { Authorization: `Bearer ${token}` },
        data: {
          title: `${prefix} ${String(n).padStart(2, '0')}`,
          slug: `bulk-${suffix}-${n}`,
          status: n % 2 === 0 ? 'published' : 'draft',
          blocks: [],
          price: 100 * n,
          ...(n === PAGE_SIZE + 1 ? { sku } : {}),
        },
      });
      expect(created.ok()).toBeTruthy();
    }

    // The list holds summaries only: no product is fetched with its blocks or rendered HTML
    const fullListed: string[] = [];
    page.on('request', (request) => {
      const url = new URL(request.url());
      if (
        url.pathname.endsWith('/contents/product') &&
        url.searchParams.get('view') !== 'summary'
      ) {
        fullListed.push(request.url());
      }
    });

    await openView(page, 'Products');
    const rows = page.getByRole('row').filter({ hasText: prefix });
    const pager = page.getByRole('navigation', { name: 'Pages of the list' });
    await page.getByRole('searchbox', { name: 'Search products' }).fill(prefix);
    await expect(rows).toHaveCount(PAGE_SIZE);
    await expect(pager.getByText(`1–${PAGE_SIZE} of ${PAGE_SIZE + 1}`)).toBeVisible();

    // The next page has the rest
    await pager.getByRole('button', { name: 'Next page' }).click();
    await expect(rows).toHaveCount(1);
    await expect(pager.getByRole('button', { name: 'Next page' })).toBeDisabled();
    await pager.getByRole('button', { name: 'Previous page' }).click();
    await expect(rows).toHaveCount(PAGE_SIZE);

    // A status filter narrows the matches and goes back to the first page
    await page.getByRole('combobox', { name: 'Filter by status' }).selectOption('published');
    await expect(rows).toHaveCount(Math.floor((PAGE_SIZE + 1) / 2));
    await page.getByRole('combobox', { name: 'Filter by status' }).selectOption('');

    // The SKU is searched too
    await page.getByRole('searchbox', { name: 'Search products' }).fill(sku);
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toContainText(`${prefix} ${PAGE_SIZE + 1}`);

    // Opening a product loads it whole for the editor
    await rows.first().click();
    const modal = page.getByRole('dialog');
    await expect(modal.getByLabel('Title')).toHaveValue(`${prefix} ${PAGE_SIZE + 1}`);
    await expect(modal.getByLabel('SKU')).toHaveValue(sku);
    expect(fullListed).toEqual([]);

    await page.keyboard.press('Escape');
    await page.getByRole('searchbox', { name: 'Search products' }).fill(`no-such-${suffix}`);
    await expect(page.getByText('No products match your search or filter.')).toBeVisible();
  });

  test('navigates to orders view and verifies order management interface', async ({ page }) => {
    // Open Orders view
    await openView(page, 'Orders');
    await expect(page.getByRole('heading', { name: 'Orders', level: 1 })).toBeVisible();

    // Verify presence of order status filter or empty state indicator
    const filterSelect = page.locator('select').first();
    await expect(filterSelect).toBeVisible();

    // Verify reload / refresh button is functional
    const reloadButton = page.getByRole('button', { name: /Reload|Refresh/i }).first();
    if (await reloadButton.isVisible()) {
      await reloadButton.click();
      await expect(page.getByRole('heading', { name: 'Orders', level: 1 })).toBeVisible();
    }
  });
});
