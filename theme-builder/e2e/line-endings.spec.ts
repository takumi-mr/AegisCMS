import { expect, test } from '@playwright/test';

import { resetTheme } from './fixture.mjs';
import { dirtyDot, file, openBuilder, readTheme } from './helpers.ts';

test.beforeEach(async ({ page }) => {
  resetTheme();
  await openBuilder(page);
});

// article.html is CRLF in the served copy (see fixture.mjs)
test('a CRLF file is not changed by opening it, and keeps CRLF when saved', async ({ page }) => {
  expect(readTheme('templates/article.html')).toContain('\r\n');

  await file(page, 'article.html').click();
  await expect(page.locator('.tab.active')).toContainText('article.html');
  await expect(page.locator('.cm-content')).toContainText('content_html');
  // Opened, not edited
  await expect(dirtyDot(page)).toHaveCount(0);
  await expect(page.getByRole('button', { name: /すべて保存/ })).toBeDisabled();

  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\n<!-- EDITED -->\n');
  await page.keyboard.press('Control+s');
  await expect(dirtyDot(page)).toHaveCount(0);

  const saved = readTheme('templates/article.html');
  expect(saved).toContain('EDITED');
  // No bare LF: every line break is still CRLF
  expect(saved.replace(/\r\n/g, '')).not.toContain('\n');
});
