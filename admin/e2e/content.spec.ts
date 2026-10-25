import { expect, test } from '@playwright/test';

import { signIn } from './helpers';

test.describe('Content Editor', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('creates a draft article, updates it to published, and verifies persistence', async ({
    page,
  }) => {
    const articleTitle = `E2E Article ${Date.now()}`;
    const articleSlug = `e2e-article-${Date.now()}`;

    // Click "New" to start a fresh article
    await page.getByRole('button', { name: 'New' }).click();

    // Fill Title
    const titleInput = page.getByPlaceholder('Enter post title...');
    await titleInput.fill(articleTitle);

    // Fill Slug
    const slugInput = page.getByPlaceholder('post-slug');
    await slugInput.fill(articleSlug);

    // Set Status to Draft
    const statusSelect = page.locator('select').filter({ hasText: /Draft|Published|Scheduled/ });
    await statusSelect.selectOption('draft');

    // Save article as Draft
    await page.getByRole('button', { name: 'Save' }).click();

    // Verify success toast
    await expect(page.getByText(/Saved!/)).toBeVisible();

    // Verify the article appears in the sidebar list with Draft status
    const articleButton = page
      .locator('aside')
      .getByRole('button', { name: new RegExp(articleTitle) });
    await expect(articleButton).toBeVisible();

    // Now change status to Published
    await statusSelect.selectOption('published');

    // Save article as Published
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText(/Saved!/)).toBeVisible();

    // Reload the page to ensure server persistence
    await page.reload();

    // Select the article from the sidebar list
    await page
      .locator('aside')
      .getByRole('button', { name: new RegExp(articleTitle) })
      .click();

    // Verify values remain intact
    await expect(page.getByPlaceholder('Enter post title...')).toHaveValue(articleTitle);
    await expect(page.getByPlaceholder('post-slug')).toHaveValue(articleSlug);
    await expect(statusSelect).toHaveValue('published');
  });

  test('switches editor modes and modifies content blocks', async ({ page }) => {
    await page.getByRole('button', { name: 'New' }).click();

    // Switch to Markdown mode
    await page.getByRole('tab', { name: 'Markdown' }).click();
    const markdownTextarea = page.locator('textarea');
    await expect(markdownTextarea).toBeVisible();

    await markdownTextarea.fill('## Headline via Markdown\n\nParagraph text written in markdown.');

    // Switch back to Blocks mode
    await page.getByRole('tab', { name: 'Blocks' }).click();

    // Verify the blocks were converted from markdown
    await expect(page.getByRole('textbox', { name: /Start typing your paragraph/i })).toHaveValue(
      'Paragraph text written in markdown.'
    );
  });
});
