import { expect, test } from '@playwright/test';

import { openPlugins, pluginCard, signIn } from './helpers';

// The server starts with no plugin enabled ([plugins].enabled = [] in aegis.e2e.toml) and the
// test plugins of the repository's test-plugins/ installed. Cards are labelled by display name
const SHOP = 'Test Shop';
const PAGE = 'Test Page';

test.beforeEach(async ({ page }) => {
  await signIn(page);
  await openPlugins(page);
});

test('lists the installed plugins with their hooks', async ({ page }) => {
  const shop = pluginCard(page, SHOP);
  await expect(shop).toBeVisible();
  await expect(shop.getByText('Disabled', { exact: true })).toBeVisible();
  await expect(shop.getByText('calculate-order', { exact: true })).toBeVisible();

  const pageCard = pluginCard(page, PAGE);
  await expect(pageCard.getByText('render-blocks', { exact: true })).toBeVisible();
  await expect(pageCard.getByText('CSP script-src: scripts.example.com')).toBeVisible();
});

test('enables and disables a plugin without a restart', async ({ page }) => {
  const card = pluginCard(page, SHOP);

  await card.getByRole('button', { name: 'Enable' }).click();
  await expect(page.getByText('Enabled plugin "test-shop"')).toBeVisible();
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  // Saved on the server: still enabled after reloading Admin Studio
  await page.reload();
  await openPlugins(page);
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  page.once('dialog', (dialog) => dialog.accept());
  await card.getByRole('button', { name: 'Disable' }).click();
  await expect(page.getByText('Disabled plugin "test-shop"')).toBeVisible();
  await expect(card.getByText('Disabled', { exact: true })).toBeVisible();
});

test('keeps a plugin enabled when the disable confirmation is dismissed', async ({ page }) => {
  const card = pluginCard(page, PAGE);
  await card.getByRole('button', { name: 'Enable' }).click();
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  page.once('dialog', (dialog) => dialog.dismiss());
  await card.getByRole('button', { name: 'Disable' }).click();
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  page.once('dialog', (dialog) => dialog.accept());
  await card.getByRole('button', { name: 'Disable' }).click();
  await expect(card.getByText('Disabled', { exact: true })).toBeVisible();
});

test('saves plugin settings', async ({ page }) => {
  const card = pluginCard(page, SHOP);
  await card.getByRole('button', { name: /^Settings/ }).click();

  await page.getByPlaceholder('New key (e.g. api_key)').fill('shipping_fee');
  await page.getByPlaceholder('Value (string)').fill('700');
  await page.getByRole('button', { name: 'Add' }).click();
  await page.getByRole('button', { name: 'Save Settings' }).click();
  await expect(page.getByText("Settings for plugin 'test-shop' saved")).toBeVisible();

  await page.reload();
  await openPlugins(page);
  await expect(card.getByText('1 keys')).toBeVisible();
});

test('reorders the enabled plugins and goes back to the configuration file', async ({ page }) => {
  await expect(pluginCard(page, SHOP)).toBeVisible();
  const resetBtn = page.getByRole('button', { name: 'Use the configuration file' });
  if (await resetBtn.isVisible()) {
    page.once('dialog', (dialog) => dialog.accept());
    await resetBtn.click();
    await expect(resetBtn).toBeHidden();
  }
  for (const name of [SHOP, PAGE]) {
    const card = pluginCard(page, name);
    await expect(card).toBeVisible();
    if (await card.getByRole('button', { name: 'Enable' }).isVisible()) {
      await card.getByRole('button', { name: 'Enable' }).click();
      await expect(card.getByText('Enabled', { exact: true })).toBeVisible();
    }
  }
  const panel = page.getByRole('region', { name: 'Hook order' });
  const items = panel.getByRole('listitem');
  await expect(items).toHaveCount(2);
  await expect(items.nth(0)).toContainText(SHOP);

  await panel.getByRole('button', { name: 'Move "test-page" up' }).click();
  await expect(page.getByText('Changed the hook order')).toBeVisible();
  await expect(items.nth(0)).toContainText(PAGE);

  // Saved on the server: the hooks keep running in that order after reloading Admin Studio
  await page.reload();
  await openPlugins(page);
  await expect(items.nth(0)).toContainText(PAGE);
  await expect(panel.getByText(/Changed in Admin Studio/)).toBeVisible();

  // [plugins].enabled = [] in aegis.e2e.toml: going back disables both
  page.once('dialog', (dialog) => dialog.accept());
  await panel.getByRole('button', { name: 'Use the configuration file' }).click();
  await expect(page.getByText('Using the plugins of the configuration file')).toBeVisible();
  await expect(panel).toBeHidden();
  await expect(pluginCard(page, SHOP).getByText('Disabled', { exact: true })).toBeVisible();
  await expect(pluginCard(page, PAGE).getByText('Disabled', { exact: true })).toBeVisible();
});
