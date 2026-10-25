import { expect, test } from '@playwright/test';

import { openView, signIn } from './helpers';

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
