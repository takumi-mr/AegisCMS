import { expect, test } from '@playwright/test';

import { openSettingsSection, signIn } from './helpers';

test.describe('Notices', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates, edits, and deletes an announcement notice', async ({ page }) => {
    const suffix = Date.now();
    const message = `Scheduled maintenance at midnight ${suffix}`;
    const updatedMessage = `Updated maintenance window ${suffix}`;
    const linkUrl = 'https://status.example.com';

    await openSettingsSection(page, 'Notices');

    // 1. Create notice
    await page.getByLabel('Message of the notice').fill(message);
    await page.getByLabel('Link address (a path or an URL)').fill(linkUrl);
    await page.getByLabel('Link text').fill('System Status');
    await page.getByRole('button', { name: 'Add' }).click();

    await expect(page.getByText('Added the notice')).toBeVisible();
    await expect(page.getByText(message)).toBeVisible();
    await expect(page.getByText(linkUrl)).toBeVisible();

    // 2. Edit notice
    await page.getByRole('button', { name: `Edit the notice: ${message}` }).click();
    await page.getByLabel('Message of the notice').fill(updatedMessage);
    await page.getByRole('button', { name: 'Save' }).click();

    await expect(page.getByText('Changed the notice')).toBeVisible();
    await expect(page.getByText(updatedMessage)).toBeVisible();

    // 3. Delete notice
    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: `Delete the notice: ${updatedMessage}` }).click();

    await expect(page.getByText('Deleted the notice')).toBeVisible();
    await expect(page.getByText(updatedMessage)).toHaveCount(0);
  });
});
