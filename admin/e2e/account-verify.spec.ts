import { expect, test } from '@playwright/test';

const SERVER = 'http://127.0.0.1:18080';
const API = '**/api/v1/customers';

// The pages for the email confirmation. The shared E2E server does not require the confirmation (the other
// specs sign up and log in at once), so the answers of the customers API are replaced here: this checks what
// the page does with them. The server side (the flow with real tokens) is covered by the Rust tests.
test.describe('Email confirmation pages', () => {
  test('sign-up with a confirmation required shows the notice and does not log in', async ({
    page,
  }) => {
    let loginCalls = 0;
    await page.route(`${API}/register`, (route) =>
      route.fulfill({
        status: 202,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'verification_sent', expires_in_secs: 172800 }),
      })
    );
    await page.route(`${API}/login`, (route) => {
      loginCalls += 1;
      return route.abort();
    });

    await page.goto(`${SERVER}/account/register`);
    await page.getByLabel('Name').fill('Hanako Waiting');
    await page.getByLabel('Email').fill('waiting@example.com');
    await page.getByLabel(/^Password/).fill('customer-password-123');
    await page.getByRole('button', { name: 'Sign up' }).click();

    await expect(page.getByRole('status')).toContainText('We have sent you an email');
    await expect(page).toHaveURL(`${SERVER}/account/register`);
    expect(loginCalls, 'no login before the address is confirmed').toBe(0);
    await expect(page.getByRole('link', { name: 'Log in' })).toBeVisible();
  });

  test('a customer who has not confirmed can ask for the email again from the login page', async ({
    page,
  }) => {
    await page.route(`${API}/login`, (route) =>
      route.fulfill({
        status: 403,
        contentType: 'application/json',
        body: JSON.stringify({
          error: 'Please confirm your email address first',
          code: 'email_not_verified',
        }),
      })
    );
    let resent: unknown = null;
    await page.route(`${API}/verify/resend`, (route) => {
      resent = route.request().postDataJSON();
      return route.fulfill({
        status: 202,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'verification_sent' }),
      });
    });

    await page.goto(`${SERVER}/account/login`);
    await page.getByLabel('Email').fill('waiting@example.com');
    await page.getByLabel('Password').fill('customer-password-123');
    await page.getByRole('button', { name: 'Log in' }).click();

    await expect(page.getByRole('alert')).toContainText('Please confirm your email address first');
    await page.getByRole('button', { name: 'Send the confirmation email again' }).click();
    await expect(page.getByRole('status')).toContainText('we have sent the email again');
    expect(resent).toEqual({ email: 'waiting@example.com' });
    await expect(page).toHaveURL(`${SERVER}/account/login`);
  });

  test('the confirmation link confirms the address and leaves the token out of the address bar', async ({
    page,
  }) => {
    let sent: unknown = null;
    await page.route(`${API}/verify`, (route) => {
      sent = route.request().postDataJSON();
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'verified' }),
      });
    });

    await page.goto(`${SERVER}/account/verify?token=${'a'.repeat(64)}`);
    await expect(page.getByRole('status')).toContainText('Your email address is confirmed');
    await expect(page.getByRole('link', { name: 'Log in' })).toBeVisible();
    expect(page.url()).not.toContain('token=');
    expect(sent).toEqual({ token: 'a'.repeat(64) });
  });

  test('an invalid or missing link offers a new email', async ({ page }) => {
    await page.route(`${API}/verify`, (route) =>
      route.fulfill({
        status: 400,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'This confirmation link is invalid or has expired' }),
      })
    );
    await page.route(`${API}/verify/resend`, (route) =>
      route.fulfill({
        status: 202,
        contentType: 'application/json',
        body: JSON.stringify({ status: 'verification_sent' }),
      })
    );

    // A token the server refuses
    await page.goto(`${SERVER}/account/verify?token=${'b'.repeat(64)}`);
    await expect(page.getByRole('alert')).toContainText('invalid or has expired');
    await page.getByLabel('Email').fill('waiting@example.com');
    await page.getByRole('button', { name: 'Send the confirmation email again' }).click();
    await expect(page.getByRole('status')).toContainText('we have sent the email again');

    // No token at all
    await page.goto(`${SERVER}/account/verify`);
    await expect(page.getByRole('alert')).toContainText('invalid or has expired');
    await expect(page.getByLabel('Email')).toBeVisible();
  });
});
