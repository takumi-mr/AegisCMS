import { expect, test } from '@playwright/test';

import { E2E_USERNAME } from './credentials.mjs';
import { openSettingsSection, signIn } from './helpers';

test.describe('User Management', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
    await openSettingsSection(page, 'Users');
  });

  test('displays existing admin and creates a new user', async ({ page }) => {
    // Initial admin should be visible
    await expect(page.getByText(`@${E2E_USERNAME}`)).toBeVisible();

    const newUsername = `editor_${Date.now()}`;
    const newDisplayName = `Test Editor ${Date.now()}`;

    // Click "New user" button
    await page.getByRole('button', { name: /New user/i }).click();

    // Verify modal is displayed
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();

    // Fill form
    await modal.getByLabel('Username').fill(newUsername);
    await modal.getByLabel('Display name').fill(newDisplayName);
    await modal.getByLabel('Initial password').fill('TestPassword123!');
    await modal.getByLabel('Role').selectOption('editor');

    // Submit
    await modal.getByRole('button', { name: 'Create user' }).click();

    // Verify toast notification and user row in the list
    await expect(page.getByText(`Created user "${newUsername}"`)).toBeVisible();
    await expect(page.getByText(`@${newUsername}`)).toBeVisible();
    await expect(page.getByText(newDisplayName)).toBeVisible();
  });

  test('invites a new user by email', async ({ page }) => {
    const inviteEmail = `invite_${Date.now()}@example.com`;

    // Click "Invite user" button
    await page.getByRole('button', { name: /Invite user/i }).click();

    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible();

    await modal.getByLabel('Email').fill(inviteEmail);
    await modal.getByRole('button', { name: 'Send invitation' }).click();

    // Verify invitation toast
    await expect(page.getByText(`Sent an invitation to ${inviteEmail}`)).toBeVisible();
  });
});
