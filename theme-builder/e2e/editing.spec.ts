import { expect, test } from '@playwright/test';

import { resetTheme } from './fixture.mjs';
import {
  cursorInContentBlock,
  dirtyDot,
  file,
  openBuilder,
  preview,
  readTheme,
  writeTheme,
} from './helpers.ts';

test.beforeEach(async ({ page }) => {
  resetTheme();
  await openBuilder(page);
});

test('lists the theme files and opens index.html with its preview', async ({ page }) => {
  for (const name of ['theme.json', 'base.html', 'article.html', 'page.html', 'style.css']) {
    await expect(file(page, name)).toBeVisible();
  }
  await expect(page.getByRole('button', { name: /すべて保存/ })).toBeDisabled();
});

test('shows an unsaved edit in the preview without touching the file', async ({ page }) => {
  await cursorInContentBlock(page);
  await page.keyboard.type('\n<p id="live">LIVE-EDIT</p>');

  await expect(preview(page).locator('#live')).toHaveText('LIVE-EDIT');
  await expect(dirtyDot(page)).toBeVisible();
  expect(readTheme('templates/index.html')).not.toContain('LIVE-EDIT');
});

test('saves with Ctrl+S', async ({ page }) => {
  await cursorInContentBlock(page);
  await page.keyboard.type('\n<p id="saved">SAVED-EDIT</p>');
  await expect(dirtyDot(page)).toBeVisible();

  await page.keyboard.press('Control+s');
  await expect(dirtyDot(page)).toHaveCount(0);
  expect(readTheme('templates/index.html')).toContain('SAVED-EDIT');
});

test('reports a syntax error with its line, keeps the last good page, and recovers', async ({
  page,
}) => {
  await cursorInContentBlock(page);
  await page.keyboard.type('\n{% if %}');

  const problem = page.locator('.problems .problem').first();
  await expect(problem).toContainText(/index\.html:\d+/);
  await expect(page.locator('.cm-lintRange-error')).not.toHaveCount(0);
  // The frame still shows the last page that rendered
  await expect(page.locator('iframe.stale')).toHaveCount(1);
  await expect(preview(page).locator('h1')).toHaveText('E2E-HOME');

  // A click on the problem puts the cursor on the line with the error
  await problem.click();
  await expect(page.locator('.cm-activeLine')).toContainText('{% if %}');

  await page.keyboard.press('Shift+End');
  await page.keyboard.press('Delete');
  await expect(page.locator('.problems .problem')).toHaveCount(0);
  await expect(page.locator('iframe.stale')).toHaveCount(0);
});

test('points at the parent template where the error is', async ({ page }) => {
  // A template that fails while rendering, reached through `extends`
  writeTheme(
    'templates/base.html',
    '<body>\n{% block content %}{% endblock %}\n{{ 1 / 0 }}\n</body>'
  );
  await page.reload();
  await expect(page.locator('.problems .problem')).toContainText(/base\.html:3/);
});

test('changes the width of the preview', async ({ page }) => {
  await page.getByRole('button', { name: '390px' }).click();
  const width = await page
    .locator('.stage iframe')
    .evaluate((el) => Math.round(el.getBoundingClientRect().width));
  expect(width).toBe(390);
});

test('shows unsaved CSS in the preview, and keeps links inside the frame', async ({ page }) => {
  await file(page, 'style.css').click();
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type('\nbody{background:rgb(1,2,3)}');

  await expect(preview(page).locator('body')).toHaveCSS('background-color', 'rgb(1, 2, 3)');
  expect(readTheme('assets/style.css')).not.toContain('rgb(1,2,3)');

  // A click on a link does not take the frame to another page
  await preview(page).locator('#home-link').click();
  await expect(preview(page).locator('h1')).toHaveText('E2E-HOME');
});

test('follows a change made by another editor', async ({ page }) => {
  await file(page, 'page.html').click();
  await expect(page.locator('.tab.active')).toContainText('page.html');

  writeTheme('templates/page.html', `${readTheme('templates/page.html')}<!-- FROM-OUTSIDE -->\n`);
  await expect(page.locator('.cm-content')).toContainText('FROM-OUTSIDE');
  await expect(dirtyDot(page)).toHaveCount(0);
});

test('warns about a change on disk while the file has unsaved edits', async ({ page }) => {
  await cursorInContentBlock(page);
  await page.keyboard.type('MINE');

  writeTheme('templates/index.html', `${readTheme('templates/index.html')}<!-- THEIRS -->\n`);
  const notice = page.locator('.editing .notice');
  await expect(notice).toContainText('ディスク上のファイルが変わりました');
  // The edit is kept until the user chooses
  await expect(page.locator('.cm-content')).toContainText('MINE');

  await notice.getByRole('button').click();
  await expect(page.locator('.cm-content')).toContainText('THEIRS');
  await expect(page.locator('.cm-content')).not.toContainText('MINE');
  await expect(dirtyDot(page)).toHaveCount(0);
});

test('checks every sample', async ({ page }) => {
  await page.getByRole('button', { name: 'すべてのサンプルを確認' }).click();
  await expect(page.locator('.problems')).toContainText('全 3 サンプルが描画できました');

  writeTheme('templates/page.html', '{% if %}');
  await page.getByRole('button', { name: 'すべてのサンプルを確認' }).click();
  await expect(page.locator('.problems .problem')).toContainText('[page]');
});

test('creates a new sample file, and refuses paths outside the theme', async ({ page }) => {
  await page.getByRole('button', { name: '新しいファイル' }).click();
  const path = page.getByLabel('新しいファイルのパス');
  const create = page.getByRole('button', { name: '作成', exact: true });

  for (const bad of ['config/x.json', 'templates/../x.html', 'templates/a.png', 'index.html']) {
    await path.fill(bad);
    await expect(create).toBeDisabled();
  }
  // A name that exists already
  await path.fill('templates/index.html');
  await expect(create).toBeDisabled();

  await path.fill('samples/wide.json');
  await create.click();
  await expect(file(page, 'wide.json')).toBeVisible();
  expect(readTheme('samples/wide.json')).toBe('');
});

test('previews a sample file that is not saved yet', async ({ page }) => {
  await page.getByRole('button', { name: '新しいファイル' }).click();
  await page.getByLabel('新しいファイルのパス').fill('samples/page.json');
  await page.getByRole('button', { name: '作成', exact: true }).click();
  await expect(page.locator('.tab.active')).toContainText('page.json');

  await page.locator('.cm-content').click();
  await page.keyboard.type('{"title":"FROM-SAMPLE"}');
  await expect(preview(page).locator('h1')).toHaveText('FROM-SAMPLE');
  expect(readTheme('samples/page.json')).toBe('');
});

test('renames and deletes a file', async ({ page }) => {
  // 新しいファイル作成
  await page.getByRole('button', { name: '新しいファイル' }).click();
  await page.getByLabel('新しいファイルのパス').fill('templates/temp.html');
  await page.getByRole('button', { name: '作成', exact: true }).click();
  await expect(file(page, 'temp.html')).toBeVisible();

  // 改名 (rename)
  const item = page.locator('.file-item').filter({ hasText: 'temp.html' });
  await item.hover();
  await item.getByRole('button', { name: /改名/ }).click();
  const renameInput = page.getByLabel('改名後のパス');
  await expect(renameInput).toHaveValue('templates/temp.html');
  await renameInput.fill('templates/renamed.html');
  await page.getByRole('button', { name: '改名を確定' }).click();

  await expect(file(page, 'renamed.html')).toBeVisible();
  await expect(file(page, 'temp.html')).not.toBeVisible();
  expect(readTheme('templates/renamed.html')).toBe('');

  // 削除 (delete)
  page.on('dialog', (dialog) => dialog.accept());
  const renamedItem = page.locator('.file-item').filter({ hasText: 'renamed.html' });
  await renamedItem.hover();
  await renamedItem.getByRole('button', { name: /削除/ }).click();

  await expect(file(page, 'renamed.html')).not.toBeVisible();
  expect(() => readTheme('templates/renamed.html')).toThrow();
});
test('edits theme.json via manifest form and switches to raw JSON', async ({ page }) => {
  await file(page, 'theme.json').click();
  await expect(page.locator('.manifest-editor')).toBeVisible();

  const nameInput = page.getByLabel(/テーマ名/);
  await expect(nameInput).toHaveValue('E2E Theme');

  await nameInput.fill('Updated E2E Theme');
  await expect(dirtyDot(page)).toBeVisible();

  await page.getByRole('button', { name: 'JSON', exact: true }).click();
  await expect(page.locator('.manifest-editor')).not.toBeVisible();
  await expect(page.locator('.cm-content')).toContainText('"name": "Updated E2E Theme"');

  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+s');
  await expect(dirtyDot(page)).toHaveCount(0);
  expect(readTheme('theme.json')).toContain('"Updated E2E Theme"');

  await page.getByRole('button', { name: 'フォーム', exact: true }).click();
  await expect(page.locator('.manifest-editor')).toBeVisible();
  await expect(page.getByLabel(/テーマ名/)).toHaveValue('Updated E2E Theme');
});
