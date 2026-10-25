import { expect, test, type Page } from '@playwright/test';

import { signIn } from './helpers';

// The server has [notifications] enabled with the log channel (aegis.e2e.toml), and keeps the
// queue in its database
const openNotifications = async (page: Page) => {
  await page
    .getByRole('navigation', { name: 'Views' })
    .getByRole('button', { name: 'Settings' })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name: 'Notifications' })
    .click();
  await expect(page.getByRole('heading', { name: 'Notifications', level: 1 })).toBeVisible();
};

test('an admin sees the delivery setup and sends a test notification', async ({ page }) => {
  await signIn(page);
  await openNotifications(page);
  await expect(page.getByText('Database (kept across restarts, up to 3 attempts)')).toBeVisible();
  await expect(page.getByText('No failed notifications.')).toBeVisible();

  await page.getByLabel('Email address').fill('you@example.com');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('sent "Test notification from AegisCMS"')).toBeVisible();
});

test('an admin edits a template with a live preview and resets it', async ({ page }) => {
  await signIn(page);
  await openNotifications(page);
  await page.getByLabel('Template', { exact: true }).selectOption('test');

  const editor = page.getByRole('group', { name: 'test.subject' });
  const subject = editor.getByRole('textbox');
  await expect(subject).toHaveValue(/Test notification/);
  await subject.fill('E2E check for {{ site_name }}');
  await expect(page.getByText('Unsaved')).toBeVisible();
  // The preview uses the unsaved edit
  await expect(page.getByText('E2E check for AegisCMS', { exact: true })).toBeVisible();
  await expect(
    page.frameLocator('iframe[title="HTML email preview"]').locator('body')
  ).toContainText('If you are reading this, notifications are delivered.');

  await editor.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Saved test.subject')).toBeVisible();
  await expect(editor.getByText('Edited', { exact: true })).toBeVisible();

  // Test sends use the saved template
  await page.getByLabel('Email address').fill('you@example.com');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('sent "E2E check for AegisCMS"')).toBeVisible();

  page.once('dialog', (dialog) => dialog.accept());
  await editor.getByRole('button', { name: 'Reset' }).click();
  await expect(page.getByText('test.subject reset')).toBeVisible();
  await expect(editor.getByText('Built-in', { exact: true })).toBeVisible();
  await expect(subject).toHaveValue(/Test notification/);
});
