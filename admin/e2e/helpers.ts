import { expect, type Page } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';

/** Signs in as the E2E admin from the login screen */
export const signIn = async (page: Page) => {
  await page.goto('./');
  await page.getByLabel('Username').fill(E2E_USERNAME);
  await page.getByLabel('Password').fill(E2E_PASSWORD);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.locator('header')).toBeVisible();
};

/** Opens a top-level view from the Views navigation tab */
export const openView = async (
  page: Page,
  viewName: 'Editor' | 'Schedule' | 'Products' | 'Orders' | 'Settings'
) => {
  await page
    .getByRole('navigation', { name: 'Views' })
    .getByRole('button', { name: viewName })
    .click();
};

/** Opens a section under Settings (e.g. 'Themes', 'Users', 'Plugins') */
export const openSettingsSection = async (page: Page, sectionName: string) => {
  await openView(page, 'Settings');
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name: sectionName })
    .click();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
};

/** Opens Settings → Plugins (the app starts on the editor, also after a reload) */
export const openPlugins = async (page: Page) => {
  await openSettingsSection(page, 'Plugins');
};

/** The card of an installed plugin, by its display name (`display_name`, or `name` without one) */
export const pluginCard = (page: Page, displayName: string) =>
  page.getByRole('article', { name: displayName, exact: true });
