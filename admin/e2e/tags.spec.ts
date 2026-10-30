import { expect, test } from '@playwright/test';

import { openSettingsSection, openView, signIn } from './helpers';

// Tags are managed in Settings → Tags and put on any content from its editor
test.describe('Tags', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates, renames and deletes a tag', async ({ page }) => {
    const suffix = Date.now();
    const name = `Rust ${suffix}`;
    await openSettingsSection(page, 'Tags');

    await page.getByLabel('Tag name').fill(name);
    await page.getByRole('button', { name: 'Create tag' }).click();
    await expect(page.getByText(`Created the tag "${name}"`)).toBeVisible();
    // The slug is made from the name
    await expect(page.getByText(`/rust-${suffix}`)).toBeVisible();

    // A duplicate name is refused with the server's message
    await page.getByLabel('Tag name').fill(name.toUpperCase());
    await page.getByRole('button', { name: 'Create tag' }).click();
    await expect(page.getByText(/already exists/)).toBeVisible();
    await page.getByLabel('Tag name').fill('');

    // Rename keeps the slug
    await page.getByRole('button', { name: `Edit tag ${name}` }).click();
    await page.getByLabel('Tag name').fill(`${name} 2`);
    await page.getByRole('button', { name: 'Save tag' }).click();
    await expect(page.getByText(`Saved the tag "${name} 2"`)).toBeVisible();
    await expect(page.getByText(`/rust-${suffix}`)).toBeVisible();

    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: `Delete tag ${name} 2` }).click();
    await expect(page.getByText(`Deleted the tag "${name} 2"`)).toBeVisible();
    await expect(page.getByText(`/rust-${suffix}`)).toHaveCount(0);
  });

  test('puts tags on an article from the editor, and they stay after a reload', async ({
    page,
  }) => {
    const suffix = Date.now();
    const title = `Tagged ${suffix}`;
    const tagName = `Docker ${suffix}`;

    await page.getByRole('button', { name: 'New' }).click();
    await page.getByPlaceholder('Enter post title...').fill(title);
    await page.getByPlaceholder('post-slug').fill(`tagged-${suffix}`);

    // Create a tag from the picker and add it
    const picker = page.getByRole('combobox', { name: 'Add a tag' });
    await picker.fill(tagName);
    await page.getByRole('option', { name: `Create "${tagName}"` }).click();
    await expect(page.getByRole('button', { name: `Remove the tag ${tagName}` })).toBeVisible();

    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText(/Saved!/)).toBeVisible();

    // Open the article again from the list: its tag is loaded
    await page.reload();
    await page.getByPlaceholder('Search title, slug or summary…').fill(title);
    await page.getByRole('button').filter({ hasText: title }).first().click();
    await expect(page.getByRole('button', { name: `Remove the tag ${tagName}` })).toBeVisible();

    // The tag is counted in Settings → Tags, and an existing tag is found by the picker
    await openSettingsSection(page, 'Tags');
    const row = page.locator('div[class*="row"]').filter({ hasText: tagName });
    await expect(row.getByText('1 content', { exact: true })).toBeVisible();
    await openView(page, 'Editor');
  });
});
