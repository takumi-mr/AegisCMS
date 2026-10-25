import { expect, test } from '@playwright/test';

import { openSettingsSection, signIn } from './helpers';

test.describe('Theme Manager', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
    await openSettingsSection(page, 'Themes');
  });

  test('lists installed themes and switches active theme', async ({ page }) => {
    // Both Aegis Horizon (default) and Nordic Minimal Editorial should be listed
    const horizonHeading = page.getByRole('heading', { name: 'Aegis Horizon', level: 2 });
    const editorialHeading = page.getByRole('heading', {
      name: 'Nordic Minimal Editorial',
      level: 2,
    });

    await expect(horizonHeading).toBeVisible();
    await expect(editorialHeading).toBeVisible();

    // Aegis Horizon card is initially active
    const horizonCard = page.locator('div[class*="card"]').filter({ has: horizonHeading }).first();
    await expect(horizonCard.getByText('Currently Active')).toBeVisible();

    // Switch to Nordic Minimal Editorial
    const editorialCard = page
      .locator('div[class*="card"]')
      .filter({ has: editorialHeading })
      .first();
    const activateBtn = editorialCard.getByRole('button', { name: 'Activate Theme' });
    await expect(activateBtn).toBeVisible();
    await activateBtn.click();

    // Verify activation toast
    await expect(
      page.getByText('Activated "Nordic Minimal Editorial" successfully!')
    ).toBeVisible();
    await expect(editorialCard.getByText('Currently Active')).toBeVisible();

    // Reload page to verify persistence
    await page.reload();
    await openSettingsSection(page, 'Themes');

    const editorialCardAfterReload = page
      .locator('div[class*="card"]')
      .filter({ has: page.getByRole('heading', { name: 'Nordic Minimal Editorial', level: 2 }) })
      .first();
    await expect(editorialCardAfterReload.getByText('Currently Active')).toBeVisible();

    // Revert back to Aegis Horizon
    const horizonCardRevert = page
      .locator('div[class*="card"]')
      .filter({ has: page.getByRole('heading', { name: 'Aegis Horizon', level: 2 }) })
      .first();
    await horizonCardRevert.getByRole('button', { name: 'Activate Theme' }).click();
    await expect(page.getByText('Activated "Aegis Horizon" successfully!')).toBeVisible();
    await expect(horizonCardRevert.getByText('Currently Active')).toBeVisible();
  });
});
