import { expect, test } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';

test.describe('Authentication and Session', () => {
  test('signs in successfully and signs out', async ({ page }) => {
    await page.goto('./');

    // Fill credentials
    await page.getByLabel('Username').fill(E2E_USERNAME);
    await page.getByLabel('Password').fill(E2E_PASSWORD);
    await page.getByRole('button', { name: 'Sign in' }).click();

    // Verify successful login
    await expect(page.getByRole('navigation', { name: 'Views' })).toBeVisible();
    await expect(page.getByText(E2E_USERNAME)).toBeVisible();

    // Sign out
    await page.getByRole('button', { name: 'Sign out' }).click();

    // Verify redirected back to login screen with sign-out notice
    await expect(page.getByRole('button', { name: 'Sign in' })).toBeVisible();
    await expect(page.getByRole('status')).toBeVisible();
  });

  test('shows an error message for invalid credentials', async ({ page }) => {
    await page.goto('./');

    await page.getByLabel('Username').fill(E2E_USERNAME);
    await page.getByLabel('Password').fill('wrong-password-xyz');
    await page.getByRole('button', { name: 'Sign in' }).click();

    await expect(page.getByText('Invalid username or password.')).toBeVisible();
  });

  test('locks out a user after repeated failed attempts', async ({ page }) => {
    await page.goto('./');

    const testUser = 'lockout-test-user';
    // Max failures is set to 3 in aegis.e2e.toml
    for (let i = 0; i < 3; i++) {
      await page.getByLabel('Username').fill(testUser);
      await page.getByLabel('Password').fill('incorrect-password');
      await page.getByRole('button', { name: 'Sign in' }).click();
      await expect(page.getByText('Invalid username or password.')).toBeVisible();
    }

    // 4th attempt should be rate limited (429 locked)
    await page.getByLabel('Username').fill(testUser);
    await page.getByLabel('Password').fill('incorrect-password');
    await page.getByRole('button', { name: 'Sign in' }).click();

    await expect(
      page.getByText(/Too many failed login attempts\. Try again in \d+ minute/)
    ).toBeVisible();
  });
});
