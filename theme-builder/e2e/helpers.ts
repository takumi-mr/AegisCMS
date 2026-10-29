import { expect, type Page } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { THEME_DIR } from './fixture.mjs';

export const readTheme = (file: string) => readFileSync(join(THEME_DIR, file), 'utf8');
export const writeTheme = (file: string, text: string) =>
  writeFileSync(join(THEME_DIR, file), text);

/** Opens Theme Builder and waits for the first file and the first preview */
export async function openBuilder(page: Page) {
  await page.goto('/');
  await expect(page.locator('.cm-content')).toBeVisible();
  await expect(page.locator('.tab.active')).toContainText('index.html');
  await expect(preview(page).locator('h1')).toHaveText('E2E-HOME');
}

export const preview = (page: Page) => page.frameLocator('iframe[title="プレビュー"]');

export const file = (page: Page, name: string) =>
  page.locator('.files .file', { hasText: name }).first();

/** The cursor at the end of line 3 of index.html (`{% block content %}`): inside the block */
export async function cursorInContentBlock(page: Page) {
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
}

/** The tab of the open file has the "unsaved" dot */
export const dirtyDot = (page: Page) => page.locator('.tab.active .dot');
