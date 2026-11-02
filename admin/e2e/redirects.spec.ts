import { expect, test } from '@playwright/test';

import { openSettingsSection, signIn } from './helpers';

test.describe('Redirects', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates, searches, edits and deletes a redirect', async ({ page }) => {
    const suffix = Date.now();
    const oldPath = `/legacy-path-${suffix}`;
    const targetPath = `/new-destination-${suffix}`;
    const updatedTarget = `/updated-destination-${suffix}`;

    await openSettingsSection(page, 'Redirects');

    // 1. Create redirect
    await page.getByLabel('Old path').fill(oldPath);
    await page.getByLabel('New path or URL').fill(targetPath);
    await page.getByRole('button', { name: 'Add' }).click();

    await expect(page.getByText(`Added a redirect from ${oldPath}`)).toBeVisible();
    await expect(page.getByText(oldPath, { exact: true })).toBeVisible();
    await expect(page.getByText(targetPath)).toBeVisible();

    // 2. Reject duplicate
    await page.getByLabel('Old path').fill(oldPath);
    await page.getByLabel('New path or URL').fill('/another-destination');
    await page.getByRole('button', { name: 'Add' }).click();
    await expect(page.getByText(/already exists|duplicate/i)).toBeVisible();

    // Clear form
    await page.getByLabel('Old path').fill('');
    await page.getByLabel('New path or URL').fill('');

    // 3. Search for the redirect
    await page.getByLabel('Search redirects').fill(`legacy-path-${suffix}`);
    await expect(page.getByText(oldPath, { exact: true })).toBeVisible();

    // 4. Edit redirect
    await page.getByRole('button', { name: `Edit the redirect from ${oldPath}` }).click();
    await page.getByLabel('New path or URL').fill(updatedTarget);
    await page.getByRole('button', { name: 'Save' }).click();

    await expect(page.getByText(`Changed the redirect from ${oldPath}`)).toBeVisible();
    await expect(page.getByText(updatedTarget)).toBeVisible();

    // 5. Delete redirect
    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: `Delete the redirect from ${oldPath}` }).click();
    await expect(page.getByText(`Deleted the redirect from ${oldPath}`)).toBeVisible();
    await expect(page.getByText(oldPath)).toHaveCount(0);
  });
});
