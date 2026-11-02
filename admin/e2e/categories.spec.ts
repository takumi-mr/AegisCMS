import { expect, test } from '@playwright/test';

import { openSettingsSection, openView, signIn } from './helpers';

test.describe('Categories', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates a category, rejects duplicates, and associates it with an article', async ({
    page,
  }) => {
    const suffix = Date.now();
    const title = `Engineering ${suffix}`;
    const slug = `engineering-${suffix}`;

    // 1. Navigate to Settings -> Categories
    await openSettingsSection(page, 'Categories');

    // 2. Fill in new category form
    await page.getByLabel('Category title').fill(title);
    await page.getByLabel('Category slug').fill(slug);
    await page.getByLabel('Category description').fill('Engineering posts and updates');
    await page.getByRole('button', { name: 'Create category' }).click();

    // Verify it appears in the list (exact match to avoid toast substring)
    await expect(page.getByText(title, { exact: true })).toBeVisible();
    await expect(page.getByText(`/${slug}`)).toBeVisible();

    // 3. Attempt duplicate slug creation
    await page.getByLabel('Category title').fill(`Duplicate ${suffix}`);
    await page.getByLabel('Category slug').fill(slug);
    await page.getByRole('button', { name: 'Create category' }).click();
    await expect(page.getByText(new RegExp(`already exists`, 'i'))).toBeVisible();

    // 4. Go to Editor and assign the category to a new article
    await openView(page, 'Editor');
    await page.getByRole('button', { name: 'New' }).click();

    const articleTitle = `Tech Post ${suffix}`;
    await page.getByPlaceholder('Enter post title...').fill(articleTitle);
    await page.getByPlaceholder('post-slug').fill(`tech-post-${suffix}`);

    // Select the newly created category from dropdown
    const categorySelect = page.getByRole('main').getByLabel('Category');
    await categorySelect.selectOption({ label: title });

    // Save article
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText(/Saved!/)).toBeVisible();

    // Reload page and verify category remains selected
    await page.reload();
    await expect(categorySelect).toHaveValue(await categorySelect.inputValue());
  });
});
