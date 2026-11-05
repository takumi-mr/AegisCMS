import { expect, test } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { openView, signIn } from './helpers';

const SERVER = 'http://127.0.0.1:18080';

// The Customers screen: a customer who signed up and paid an order appears with the order, and an editor can
// suspend the account (the customer cannot log in any more) and reactivate it.
test.describe('Customers screen', () => {
  test('shows a customer with the order and suspends and reactivates the account', async ({
    page,
    playwright,
  }) => {
    const suffix = Date.now();
    const email = `admin-view-${suffix}@example.com`;
    const password = 'customer-password-123';
    const slug = `customer-mug-${suffix}`;

    const admin = await playwright.request.newContext();
    const login = await admin.post(`${SERVER}/api/v1/auth/session`, {
      data: { username: E2E_USERNAME, password: E2E_PASSWORD },
    });
    const token = (await login.json()).id as string;
    const auth = { Authorization: `Bearer ${token}` };
    const product = await admin.post(`${SERVER}/api/v1/contents/product`, {
      headers: auth,
      data: {
        title: `Customer Mug ${suffix}`,
        slug,
        status: 'published',
        blocks: [],
        fields: { price: 2000, stock: 5 },
      },
    });
    expect(product.ok(), await product.text()).toBeTruthy();

    // The customer signs up, logs in (the cookie) and buys the mug
    const shopper = await playwright.request.newContext();
    const registered = await shopper.post(`${SERVER}/api/v1/customers/register`, {
      data: { email, password, name: 'Admin View Hanako' },
    });
    expect(registered.ok(), await registered.text()).toBeTruthy();
    const loggedIn = await shopper.post(`${SERVER}/api/v1/customers/login`, {
      data: { email, password },
    });
    expect(loggedIn.ok(), await loggedIn.text()).toBeTruthy();
    const checkout = await shopper.post(`${SERVER}/api/v1/shop/checkout`, {
      data: { items: [{ product: slug, quantity: 1 }], email, name: 'Admin View Hanako' },
    });
    expect(checkout.ok(), await checkout.text()).toBeTruthy();
    const { order } = await checkout.json();
    const paid = await admin.post(`${SERVER}/api/v1/orders/${order.id}/status`, {
      headers: auth,
      data: { status: 'paid' },
    });
    expect(paid.ok(), await paid.text()).toBeTruthy();

    await signIn(page);
    await openView(page, 'Customers');
    await expect(page.getByRole('heading', { name: 'Customers', level: 1 })).toBeVisible();
    const row = page.getByRole('row').filter({ hasText: email });
    await expect(row).toContainText('Admin View Hanako');
    await expect(row).toContainText('Active');
    await row.click();

    const modal = page.getByRole('dialog');
    await expect(modal.getByText(order.number)).toBeVisible();
    await expect(modal.getByText('Paid in total')).toBeVisible();

    // Suspend: the dialog asks first; the customer cannot log in until the account is active again
    page.once('dialog', (dialog) => dialog.accept());
    await modal.getByRole('button', { name: 'Suspend' }).click();
    await expect(page.getByText('The customer is suspended')).toBeVisible();
    await expect(modal.getByRole('button', { name: 'Reactivate' })).toBeVisible();
    const refused = await playwright.request
      .newContext()
      .then((ctx) => ctx.post(`${SERVER}/api/v1/customers/login`, { data: { email, password } }));
    expect(refused.ok()).toBe(false);

    await modal.getByRole('button', { name: 'Reactivate' }).click();
    await expect(page.getByText('The customer is active again')).toBeVisible();
    const accepted = await playwright.request
      .newContext()
      .then((ctx) => ctx.post(`${SERVER}/api/v1/customers/login`, { data: { email, password } }));
    expect(accepted.ok()).toBe(true);
  });
});
