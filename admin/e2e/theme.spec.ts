import { expect, test, type Page } from '@playwright/test';

import { signIn } from './helpers';

// The Admin Studio color theme: light, dark, or follow the OS. Set by `data-theme` on <html>
// (public/theme-init.js before the first paint, then src/theme)
const theme = (page: Page) => page.evaluate(() => document.documentElement.dataset.theme);
const background = (page: Page) =>
  page.evaluate(() => getComputedStyle(document.body).backgroundColor);

const DARK_BACKGROUND = 'rgb(7, 9, 14)';
const LIGHT_BACKGROUND = 'rgb(241, 245, 249)';

test.describe('Color theme', () => {
  test('follows the OS by default, and the sign-in screen can switch it', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('./');
    await expect.poll(() => theme(page)).toBe('dark');
    expect(await background(page)).toBe(DARK_BACKGROUND);

    // Changing the OS setting applies while the page is open
    await page.emulateMedia({ colorScheme: 'light' });
    await expect.poll(() => theme(page)).toBe('light');
    expect(await background(page)).toBe(LIGHT_BACKGROUND);

    // Before signing in, the choice is already available
    const select = page.getByRole('combobox', { name: 'Theme' });
    await expect(select).toHaveValue('system');
    await select.selectOption('dark');
    await expect.poll(() => theme(page)).toBe('dark');
    expect(await background(page)).toBe(DARK_BACKGROUND);
  });

  test('keeps the choice over the OS setting, across a reload and the sign-in', async ({
    page,
  }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await signIn(page);
    await expect.poll(() => theme(page)).toBe('light');

    const select = page.locator('header').getByRole('combobox', { name: 'Theme' });
    await select.selectOption('dark');
    await expect.poll(() => theme(page)).toBe('dark');
    expect(await background(page)).toBe(DARK_BACKGROUND);

    await page.reload();
    await expect(page.locator('header')).toBeVisible();
    expect(await theme(page)).toBe('dark');
    await expect(select).toHaveValue('dark');

    // "System" goes back to following the OS, and the OS can still be changed afterwards
    await select.selectOption('system');
    await expect.poll(() => theme(page)).toBe('light');
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect.poll(() => theme(page)).toBe('dark');
  });

  test('is applied before the application loads, so the page does not flash', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('./');
    await page.getByRole('combobox', { name: 'Theme' }).selectOption('light');
    await expect.poll(() => theme(page)).toBe('light');

    // Without any of the application's scripts, the chosen theme is already on the page
    await page.route('**/assets/*.js', (route) => route.abort());
    await page.reload();
    expect(await theme(page)).toBe('light');
    expect(await background(page)).toBe(LIGHT_BACKGROUND);
  });

  test('the light theme keeps the main text readable on its background', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await signIn(page);
    const colors = await page.evaluate(() => {
      const style = getComputedStyle(document.body);
      return { text: style.color, background: style.backgroundColor };
    });
    // Dark text on a light page (not the white text of the dark theme)
    expect(colors.text).toBe('rgb(15, 23, 42)');
    expect(colors.background).toBe(LIGHT_BACKGROUND);
  });
});
