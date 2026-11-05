import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

import { E2E_PASSWORD, E2E_USERNAME } from './credentials.mjs';

const SERVER = 'http://127.0.0.1:18080';

// Product reviews on the product page: a logged-in member writes one for an order they paid, and a
// guest writes one from the link on the order page. A new review waits for approval (the default).
const adminToken = async (request: APIRequestContext): Promise<string> => {
  const login = await request.post(`${SERVER}/api/v1/auth/session`, {
    data: { username: E2E_USERNAME, password: E2E_PASSWORD },
  });
  expect(login.ok()).toBeTruthy();
  return (await login.json()).id;
};

const createProduct = async (request: APIRequestContext, token: string, slug: string) => {
  const created = await request.post(`${SERVER}/api/v1/contents/product`, {
    headers: { Authorization: `Bearer ${token}` },
    data: {
      title: `Review Mug ${slug}`,
      slug,
      status: 'published',
      blocks: [],
      fields: { price: 1500, stock: 20 },
    },
  });
  expect(created.ok()).toBeTruthy();
};

// Places an order through the shop API with the cookies of `request` (a logged-in member's, if any)
// and marks it paid as the admin; returns the order and its confirmation token
const placePaidOrder = async (
  request: APIRequestContext,
  admin: APIRequestContext,
  token: string,
  slug: string,
  email: string
) => {
  const checkout = await request.post(`${SERVER}/api/v1/shop/checkout`, {
    data: { items: [{ product: slug, quantity: 1 }], email, name: 'Hanako Yamada' },
  });
  expect(checkout.ok(), await checkout.text()).toBeTruthy();
  const { order, token: orderToken } = await checkout.json();
  const paid = await admin.post(`${SERVER}/api/v1/orders/${order.id}/status`, {
    headers: { Authorization: `Bearer ${token}` },
    data: { status: 'paid' },
  });
  expect(paid.ok(), await paid.text()).toBeTruthy();
  return { id: order.id as string, token: orderToken as string };
};

const approveLatestReview = async (request: APIRequestContext, token: string, slug: string) => {
  const list = await request.get(`${SERVER}/api/v1/admin/reviews?status=pending&limit=200`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const { reviews } = await list.json();
  const product = await request.get(`${SERVER}/api/v1/contents/product/${slug}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const productId = (await product.json()).id;
  const review = reviews.find((r: { product_id: string }) => r.product_id === productId);
  expect(review, 'the review waits for approval').toBeTruthy();
  const patched = await request.patch(`${SERVER}/api/v1/admin/reviews/${review.id}`, {
    headers: { Authorization: `Bearer ${token}` },
    data: { status: 'approved' },
  });
  expect(patched.ok()).toBeTruthy();
};

const fillReview = async (page: Page, title: string, body: string) => {
  await page.getByLabel('Rating').selectOption('4');
  await page.getByLabel('Title').fill(title);
  await page.getByLabel('Review', { exact: true }).fill(body);
  await page.getByRole('button', { name: 'Post the review' }).click();
};

test.describe('Product reviews', () => {
  test('a logged-in member reviews a paid order, and the approved review shows with its stars', async ({
    page,
  }) => {
    const suffix = Date.now();
    const slug = `review-mug-${suffix}`;
    const token = await adminToken(page.request);
    await createProduct(page.request, token, slug);

    // Not logged in: the form asks to log in
    await page.goto(`${SERVER}/products/${slug}`);
    await expect(page.getByText('There are no reviews yet.')).toBeVisible();
    await expect(page.getByText('Log in to review a product you bought.')).toBeVisible();

    // Sign up in the browser; the cookie lets the order attach to the account
    await page.goto(`${SERVER}/account/register`);
    await page.getByLabel('Name').fill('Hanako Reviewer');
    await page.getByLabel('Email').fill(`reviewer-${suffix}@example.com`);
    await page.getByLabel(/^Password/).fill('member-password-123');
    await page.getByRole('button', { name: 'Sign up' }).click();
    await expect(page).toHaveURL(`${SERVER}/account`);

    // Before the order is paid there is nothing to review
    await page.goto(`${SERVER}/products/${slug}`);
    await expect(page.getByText(/Only customers who bought this product/)).toBeVisible();

    await placePaidOrder(page.request, page.request, token, slug, `reviewer-${suffix}@example.com`);

    // The form appears for the paid order; the review waits for approval
    await page.goto(`${SERVER}/products/${slug}`);
    await expect(page.getByRole('heading', { name: 'Write a review' })).toBeVisible();
    await fillReview(page, 'Lovely mug', 'Keeps the coffee warm for an hour.');
    await expect(
      page.getByText('Thank you! Your review will appear once it has been approved.')
    ).toBeVisible();
    await expect(page.getByText('Lovely mug')).toHaveCount(0);

    // One review per order: after reloading, the form is gone
    await page.reload();
    await expect(page.getByText(/Only customers who bought this product/)).toBeVisible();

    // The shop approves it: the page shows the review, the badge and the average
    await approveLatestReview(page.request, token, slug);
    await page.reload();
    await expect(page.getByText('Lovely mug')).toBeVisible();
    await expect(page.getByText('Keeps the coffee warm for an hour.')).toBeVisible();
    await expect(page.getByText('Verified purchase')).toBeVisible();
    await expect(page.getByText('4.0 / 5 (reviews: 1)')).toBeVisible();

    // The search engines get the numbers as structured data
    const ld = await page
      .locator('script[type="application/ld+json"]')
      .evaluateAll((nodes) => nodes.map((node) => JSON.parse(node.textContent ?? '{}')));
    expect(ld[0].aggregateRating).toMatchObject({ ratingValue: 4, reviewCount: 1 });

    // The reviewer leaves: the review stays (and keeps counting), without the name
    await expect(page.getByText(/Hanako Yamada|Hanako Reviewer/)).toHaveCount(1);
    const deleted = await page.request.delete(`${SERVER}/api/v1/members/me`, {
      data: { password: 'member-password-123' },
    });
    expect(deleted.status()).toBe(204);
    await page.reload();
    await expect(page.getByText('Lovely mug')).toBeVisible();
    await expect(page.getByText(/Former customer/)).toBeVisible();
    await expect(page.getByText(/Hanako Yamada|Hanako Reviewer/)).toHaveCount(0);
    await expect(page.getByText('4.0 / 5 (reviews: 1)')).toBeVisible();
  });

  test('a guest reviews from the link on the order page, and the token leaves the address bar', async ({
    page,
    playwright,
  }) => {
    const suffix = Date.now();
    const slug = `review-guest-mug-${suffix}`;
    const admin = await playwright.request.newContext();
    const token = await adminToken(admin);
    await createProduct(admin, token, slug);

    // A guest order (no account): paid by the shop
    const guest = await playwright.request.newContext();
    const order = await placePaidOrder(guest, admin, token, slug, `guest-${suffix}@example.com`);

    // The order page links to the product page with the order and its token
    await page.goto(`${SERVER}/orders/${order.id}?token=${order.token}`);
    await page.getByRole('link', { name: 'Write a review' }).click();
    await expect(page).toHaveURL(new RegExp(`/products/${slug}`));
    // The script has read the token and taken it out of the address bar
    await expect(page.getByRole('heading', { name: 'Write a review' })).toBeVisible();
    expect(page.url()).not.toContain('token=');
    expect(page.url()).not.toContain('review_order');

    await page.getByLabel('Your name').fill('Guest Hanako');
    await fillReview(page, 'Good for a gift', 'Bought it for a friend.');
    await expect(
      page.getByText('Thank you! Your review will appear once it has been approved.')
    ).toBeVisible();

    await approveLatestReview(admin, token, slug);
    await page.goto(`${SERVER}/products/${slug}`);
    await expect(page.getByText('Good for a gift')).toBeVisible();
    await expect(page.getByText(/Guest Hanako/)).toBeVisible();

    await admin.dispose();
    await guest.dispose();
  });
});
