import { expect, test } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';
import { openView, signIn } from './helpers';

const SERVER = 'http://127.0.0.1:18080';

// The Reviews screen: a review a guest wrote waits for approval; an editor replies, approves it and the
// product page shows both. Rejecting asks for a reason.
test.describe('Reviews screen', () => {
  test('replies to a pending review, approves it and rejects another with a reason', async ({
    page,
    playwright,
  }) => {
    const suffix = Date.now();
    const slug = `admin-review-mug-${suffix}`;

    const admin = await playwright.request.newContext();
    const login = await admin.post(`${SERVER}/api/v1/auth/session`, {
      data: { username: E2E_USERNAME, password: E2E_PASSWORD },
    });
    const token = (await login.json()).id as string;
    const auth = { Authorization: `Bearer ${token}` };
    const product = await admin.post(`${SERVER}/api/v1/contents/product`, {
      headers: auth,
      data: {
        title: `Admin Review Mug ${suffix}`,
        slug,
        status: 'published',
        blocks: [],
        fields: { price: 1800, stock: 20 },
      },
    });
    expect(product.ok(), await product.text()).toBeTruthy();

    // Two guests buy the mug (paid) and each writes a review, which waits for approval
    const guest = await playwright.request.newContext();
    const write = async (n: number, title: string, body: string) => {
      const checkout = await guest.post(`${SERVER}/api/v1/shop/checkout`, {
        data: {
          items: [{ product: slug, quantity: 1 }],
          email: `guest-${n}-${suffix}@example.com`,
          name: `Guest ${n}`,
        },
      });
      expect(checkout.ok(), await checkout.text()).toBeTruthy();
      const { order, token: orderToken } = await checkout.json();
      const paid = await admin.post(`${SERVER}/api/v1/orders/${order.id}/status`, {
        headers: auth,
        data: { status: 'paid' },
      });
      expect(paid.ok(), await paid.text()).toBeTruthy();
      const posted = await guest.post(
        `${SERVER}/api/v1/products/${(await product.json()).id}/reviews`,
        {
          data: {
            order_id: order.id,
            access_token: orderToken,
            author_name: `Guest ${n}`,
            rating: 5,
            title,
            body,
          },
        }
      );
      expect(posted.ok(), await posted.text()).toBeTruthy();
    };
    await write(1, `Great mug ${suffix}`, 'Keeps the coffee hot.');
    await write(2, `Spammy ${suffix}`, 'Buy pills here.');

    await signIn(page);
    await openView(page, 'Reviews');
    await expect(page.getByRole('heading', { name: 'Reviews', level: 1 })).toBeVisible();

    // Reply to the first review, then approve it
    const good = page.getByRole('row').filter({ hasText: `Great mug ${suffix}` });
    await expect(good).toContainText('Pending');
    await good.getByRole('button', { name: 'Reply' }).click();
    const reply = page.getByRole('dialog');
    await reply.getByLabel('Reply from the shop').fill('Thank you for the kind words!');
    await reply.getByRole('button', { name: 'Save the reply' }).click();
    await expect(page.getByText('Saved the reply')).toBeVisible();
    await expect(good).toContainText('Thank you for the kind words!');
    await good.getByRole('button', { name: 'Approve' }).click();
    await expect(page.getByText('Approved 1 review(s)')).toBeVisible();
    await expect(good).toHaveCount(0);

    // Reject the second one with a reason
    const bad = page.getByRole('row').filter({ hasText: `Spammy ${suffix}` });
    await bad.getByRole('button', { name: 'Reject' }).click();
    const reason = page.getByRole('dialog');
    await reason.getByLabel('Reason (optional)').fill('Advertising');
    await reason.getByRole('button', { name: 'Reject' }).click();
    await expect(page.getByText('Rejected 1 review(s)')).toBeVisible();
    await page.getByRole('tab', { name: 'Rejected' }).click();
    await expect(page.getByRole('row').filter({ hasText: `Spammy ${suffix}` })).toContainText(
      'Reason: Advertising'
    );

    // The product page shows the approved review with the shop's reply, not the rejected one
    await page.goto(`${SERVER}/products/${slug}`);
    await expect(page.getByText(`Great mug ${suffix}`)).toBeVisible();
    await expect(page.getByText('Thank you for the kind words!')).toBeVisible();
    await expect(page.getByText(`Spammy ${suffix}`)).toHaveCount(0);
  });
});
