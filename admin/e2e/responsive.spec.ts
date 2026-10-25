import { expect, test } from '@playwright/test';

import { signIn } from './helpers';

test.describe('Responsive and Mobile Behavior', () => {
  test.use({ viewport: { width: 375, height: 667 } });

  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('toggles mobile navigation drawer and switches views', async ({ page }) => {
    const menuToggle = page.getByRole('button', { name: 'Toggle navigation menu' });
    await expect(menuToggle).toBeVisible();

    // Open mobile menu drawer
    await menuToggle.click();

    const mobileMenu = page.getByRole('dialog');
    await expect(mobileMenu).toBeVisible();

    // Click Settings from mobile drawer
    await mobileMenu.getByRole('button', { name: 'Settings' }).click();

    // Verify drawer closed and Settings view is open
    await expect(mobileMenu).toBeHidden();
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();

    // Open mobile drawer again and navigate back to Editor
    await menuToggle.click();
    await expect(mobileMenu).toBeVisible();
    await mobileMenu.getByRole('button', { name: 'Editor' }).click();
    await expect(mobileMenu).toBeHidden();
  });

  test('switches between mobile editor panes (Articles, Editor, Preview)', async ({ page }) => {
    const paneTabs = page.getByRole('tablist', { name: 'Editor panels' });
    await expect(paneTabs).toBeVisible();

    const listTab = paneTabs.getByRole('tab', { name: 'Articles' });
    const editorTab = paneTabs.getByRole('tab', { name: 'Editor' });
    const previewTab = paneTabs.getByRole('tab', { name: 'Preview' });

    // Switch to Articles (List) pane
    await listTab.click();
    await expect(listTab).toHaveAttribute('aria-selected', 'true');
    const articlesSidebar = page.locator('aside').filter({ hasText: /Articles/ });
    await expect(articlesSidebar).toBeVisible();

    // Switch to Preview pane
    await previewTab.click();
    await expect(previewTab).toHaveAttribute('aria-selected', 'true');
    await expect(articlesSidebar).toBeHidden();

    // Switch back to Editor pane
    await editorTab.click();
    await expect(editorTab).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByPlaceholder('Enter post title...')).toBeVisible();
  });
});
