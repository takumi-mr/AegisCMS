import { expect, test } from '@playwright/test';

const SERVER = 'http://127.0.0.1:18080';

// The customer account pages of the shop (/account): sign up, my page, log out and log in again.
// The page is built by /shop/account.js; the login lives in an HttpOnly cookie
test.describe('Customer account pages', () => {
  test('signs up, sees the empty order history, logs out and logs in again', async ({ page }) => {
    const suffix = Date.now();
    const email = `customer-${suffix}@example.com`;
    const password = 'customer-password-123';

    // Not logged in: my page sends the visitor to the login page
    await page.goto(`${SERVER}/account`);
    await expect(page).toHaveURL(`${SERVER}/account/login`);
    await expect(page.getByRole('heading', { name: 'Log in' })).toBeVisible();

    // Sign up
    await page.getByRole('link', { name: 'Sign up' }).click();
    await expect(page).toHaveURL(`${SERVER}/account/register`);
    await page.getByLabel('Name').fill('Hanako Test');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel(/^Password/).fill(password);
    await page.getByRole('button', { name: 'Sign up' }).click();

    // Registered and logged in: my page
    await expect(page).toHaveURL(`${SERVER}/account`);
    await expect(page.getByText('Hello, Hanako Test')).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Order history' })).toBeVisible();
    await expect(page.getByText('You have not placed an order yet.')).toBeVisible();
    await expect(page.getByText('You have not written a review yet.')).toBeVisible();

    // The token never reaches the page's scripts: the cookie is HttpOnly
    expect(await page.evaluate(() => document.cookie)).not.toContain('_customer=');
    const cookies = await page.context().cookies(SERVER);
    const session = cookies.find((cookie) => cookie.name.endsWith('_customer'));
    expect(session?.httpOnly).toBe(true);
    expect(session?.sameSite).toBe('Lax');

    // The login and sign-up pages send a logged-in customer to my page
    await page.goto(`${SERVER}/account/login`);
    await expect(page).toHaveURL(`${SERVER}/account`);

    // Edit the details
    await page.getByLabel('Name').first().fill('Hanako Renamed');
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText('Saved ✓')).toBeVisible();
    await page.reload();
    await expect(page.getByText('Hello, Hanako Renamed')).toBeVisible();

    // Log out: back to the login page, and my page is closed again
    await page.getByRole('button', { name: 'Log out' }).click();
    await expect(page).toHaveURL(`${SERVER}/account/login`);
    await page.goto(`${SERVER}/account`);
    await expect(page).toHaveURL(`${SERVER}/account/login`);

    // A wrong password is refused with the API's message
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill('not-the-password');
    await page.getByRole('button', { name: 'Log in' }).click();
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(page).toHaveURL(`${SERVER}/account/login`);

    // The right one logs in again
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Log in' }).click();
    await expect(page).toHaveURL(`${SERVER}/account`);
    await expect(page.getByText('Hello, Hanako Renamed')).toBeVisible();
  });

  test('refuses a registration with an email that is already taken', async ({ page }) => {
    const email = `taken-${Date.now()}@example.com`;
    const register = async () => {
      await page.goto(`${SERVER}/account/register`);
      await page.getByLabel('Name').fill('Someone');
      await page.getByLabel('Email').fill(email);
      await page.getByLabel(/^Password/).fill('customer-password-123');
      await page.getByRole('button', { name: 'Sign up' }).click();
    };
    await register();
    await expect(page).toHaveURL(`${SERVER}/account`);

    await page.context().clearCookies();
    await register();
    await expect(page.getByRole('alert')).toContainText('already exists');
    await expect(page).toHaveURL(`${SERVER}/account/register`);
  });

  test('asks for a reset link without telling whether the address has an account', async ({
    page,
  }) => {
    await page.goto(`${SERVER}/account/login`);
    await page.getByRole('link', { name: 'Forgot your password?' }).click();
    await expect(page).toHaveURL(`${SERVER}/account/forgot`);
    await expect(page.getByRole('heading', { name: 'Reset your password' })).toBeVisible();

    // The answer is the same for an address nobody registered
    await page.getByLabel('Email').fill(`nobody-${Date.now()}@example.com`);
    await page.getByRole('button', { name: 'Send the link' }).click();
    await expect(page.getByRole('status')).toContainText('If an account exists for that address');

    // A mistyped address is refused before anything is sent
    await page.getByLabel('Email').fill('not an email');
    await page.getByRole('button', { name: 'Send the link' }).click();
    await expect(page.getByRole('alert')).toBeVisible();
  });

  test('refuses a reset link that is missing or not valid, and takes the token out of the address bar', async ({
    page,
  }) => {
    // No token: the page says so and offers a new link
    await page.goto(`${SERVER}/account/reset`);
    await expect(page.getByText('This link is invalid or has expired.')).toBeVisible();
    await expect(page.getByRole('link', { name: 'Request a new link' })).toBeVisible();

    // A token nobody issued: the form shows the API's refusal
    await page.goto(`${SERVER}/account/reset?token=${'0'.repeat(64)}`);
    expect(page.url()).not.toContain('token=');
    await page.getByLabel(/^New password \(at least/).fill('a-brand-new-password');
    await page.getByLabel('New password (again)').fill('a-different-password-1');
    await page.getByRole('button', { name: 'Set the password' }).click();
    await expect(page.getByRole('alert')).toContainText('do not match');

    await page.getByLabel('New password (again)').fill('a-brand-new-password');
    await page.getByRole('button', { name: 'Set the password' }).click();
    await expect(page.getByRole('alert')).toContainText('invalid or has expired');
  });

  test('changing the password keeps this browser logged in', async ({ page }) => {
    const email = `changer-${Date.now()}@example.com`;
    await page.goto(`${SERVER}/account/register`);
    await page.getByLabel('Name').fill('Hanako Changer');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel(/^Password/).fill('first-password-123');
    await page.getByRole('button', { name: 'Sign up' }).click();
    await expect(page).toHaveURL(`${SERVER}/account`);

    await page.getByLabel('Current password').fill('first-password-123');
    await page.getByLabel(/^New password/).fill('second-password-456');
    await page.getByRole('button', { name: 'Change password' }).click();
    await expect(page.getByText('Saved ✓')).toBeVisible();

    // The new session cookie came with the answer: the page still works after a reload
    await page.reload();
    await expect(page.getByText('Hello, Hanako Changer')).toBeVisible();

    // Log out; only the new password logs in
    await page.getByRole('button', { name: 'Log out' }).click();
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill('first-password-123');
    await page.getByRole('button', { name: 'Log in' }).click();
    await expect(page.getByRole('alert')).toBeVisible();
    await page.getByLabel('Password').fill('second-password-456');
    await page.getByRole('button', { name: 'Log in' }).click();
    await expect(page).toHaveURL(`${SERVER}/account`);
  });
});
