import { expect, test } from '@playwright/test';

import { openView, signIn } from './helpers';

test.describe('Schedule Calendar', () => {
  test.beforeEach(async ({ page }) => {
    await signIn(page);
  });

  test('views publishing schedule, toggles between calendar and agenda, and reflects scheduled content', async ({
    page,
  }) => {
    const suffix = Date.now();
    const title = `Scheduled Launch ${suffix}`;

    // 1. Create a scheduled article in the Editor
    await page.getByRole('button', { name: 'New' }).click();
    await page.getByPlaceholder('Enter post title...').fill(title);
    await page.getByPlaceholder('post-slug').fill(`scheduled-launch-${suffix}`);

    // Change status to scheduled using the editor main area status select
    const statusSelect = page
      .getByRole('main')
      .locator('select')
      .filter({ hasText: /Draft|Published|Scheduled/ });
    await statusSelect.selectOption('scheduled');

    // Set publish_at date in the future (e.g. 2029-01-01T12:00)
    const publishAtInput = page.getByLabel('Publish date and time', { exact: true });
    await publishAtInput.fill('2029-01-01T12:00');

    // Save article
    await page.getByRole('button', { name: 'Save' }).click();
    await expect(page.getByText(/Saved!/)).toBeVisible();

    // 2. Open Schedule view
    await openView(page, 'Schedule');
    await expect(page.getByRole('heading', { name: 'Publishing Schedule' })).toBeVisible();

    // 3. Toggle between views
    const agendaTab = page.getByRole('tab', { name: 'Agenda' });
    await agendaTab.click();
    await expect(page.getByText('Publish & Unpublish Timeline')).toBeVisible();

    // The scheduled article should appear in the agenda list
    await expect(page.getByText(title)).toBeVisible();

    // Switch back to Calendar tab
    const calendarTab = page.getByRole('tab', { name: 'Calendar' });
    await calendarTab.click();
    await expect(calendarTab).toHaveAttribute('aria-selected', 'true');

    // Month navigation
    await page.getByRole('button', { name: 'Next month' }).click();
    await page.getByRole('button', { name: 'Today' }).click();
  });
});
