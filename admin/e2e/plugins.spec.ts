import { expect, test } from '@playwright/test';

import { openPlugins, pluginCard, signIn } from './helpers';

// The server starts with no plugin enabled ([plugins].enabled = [] in aegis.e2e.toml) and the
// official plugins of the repository's plugins/ installed. Cards are labelled by display name
const SHOP_RULES = 'Shop Rules (Shipping, Coupons & Tax)';
const CODE_ENHANCER = 'official-code-enhancer';

test.beforeEach(async ({ page }) => {
  await signIn(page);
  await openPlugins(page);
});

test('lists the installed plugins with their hooks', async ({ page }) => {
  const shopRules = pluginCard(page, SHOP_RULES);
  await expect(shopRules).toBeVisible();
  await expect(shopRules.getByText('Disabled', { exact: true })).toBeVisible();
  await expect(shopRules.getByText('calculate-order', { exact: true })).toBeVisible();

  const enhancer = pluginCard(page, CODE_ENHANCER);
  await expect(enhancer.getByText('transform-content-ast', { exact: true })).toBeVisible();
  await expect(enhancer.getByText(/cdn\.jsdelivr\.net/)).toBeVisible();
});

test('enables and disables a plugin without a restart', async ({ page }) => {
  const card = pluginCard(page, SHOP_RULES);

  await card.getByRole('button', { name: 'Enable' }).click();
  await expect(page.getByText('Enabled plugin "official-shop-rules"')).toBeVisible();
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  // Saved on the server: still enabled after reloading Admin Studio
  await page.reload();
  await openPlugins(page);
  await expect(card.getByText('Enabled', { exact: true })).toBeVisible();

  page.once('dialog', (dialog) => dialog.accept());
  await card.getByRole('button', { name: 'Disable' }).click();
  await expect(page.getByText('Disabled plugin "official-shop-rules"')).toBeVisible();
  await expect(card.getByText('Disabled', { exact: true })).toBeVisible();
});

test('keeps a plugin enabled when the disable confirmation is dismissed', async ({ page }) => {
  const card = pluginCard(page, CODE_ENHANCER);
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
  const card = pluginCard(page, SHOP_RULES);
  await card.getByRole('button', { name: /^Settings/ }).click();

  await page.getByPlaceholder('New key (e.g. api_key)').fill('shipping_fee');
  await page.getByPlaceholder('Value (string)').fill('700');
  await page.getByRole('button', { name: 'Add' }).click();
  await page.getByRole('button', { name: 'Save Settings' }).click();
  await expect(page.getByText("Settings for plugin 'official-shop-rules' saved")).toBeVisible();

  await page.reload();
  await openPlugins(page);
  await expect(card.getByText('1 keys')).toBeVisible();
});

test('reorders the enabled plugins and goes back to the configuration file', async ({ page }) => {
  await expect(pluginCard(page, SHOP_RULES)).toBeVisible();
  const resetBtn = page.getByRole('button', { name: 'Use the configuration file' });
  if (await resetBtn.isVisible()) {
    page.once('dialog', (dialog) => dialog.accept());
    await resetBtn.click();
    await expect(resetBtn).toBeHidden();
  }
  for (const name of [SHOP_RULES, CODE_ENHANCER]) {
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
  await expect(items.nth(0)).toContainText('Shop Rules');

  await panel.getByRole('button', { name: 'Move "official-code-enhancer" up' }).click();
  await expect(page.getByText('Changed the hook order')).toBeVisible();
  await expect(items.nth(0)).toContainText(CODE_ENHANCER);

  // Saved on the server: the hooks keep running in that order after reloading Admin Studio
  await page.reload();
  await openPlugins(page);
  await expect(items.nth(0)).toContainText(CODE_ENHANCER);
  await expect(panel.getByText(/Changed in Admin Studio/)).toBeVisible();

  // [plugins].enabled = [] in aegis.e2e.toml: going back disables both
  page.once('dialog', (dialog) => dialog.accept());
  await panel.getByRole('button', { name: 'Use the configuration file' }).click();
  await expect(page.getByText('Using the plugins of the configuration file')).toBeVisible();
  await expect(panel).toBeHidden();
  await expect(pluginCard(page, SHOP_RULES).getByText('Disabled', { exact: true })).toBeVisible();
  await expect(
    pluginCard(page, CODE_ENHANCER).getByText('Disabled', { exact: true })
  ).toBeVisible();
});
