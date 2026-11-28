import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import type { StoreUpdatesResponse } from '../types';
import { mockApi } from '../../.storybook/mockApi';
import { StoreUpdatesProvider } from './StoreUpdatesProvider';
import { StoreUpdateStatus } from './StoreUpdateStatus';

const checkedAt = '2026-11-28T03:00:00Z';

let calls = 0;
const api = (answer: StoreUpdatesResponse | { status: number; body: unknown }) => () => {
  calls = 0;
  return mockApi((req) => {
    if (req.path !== '/store/updates') return undefined;
    calls += 1;
    return answer;
  });
};

const meta = {
  title: 'Settings/StoreUpdateStatus',
  component: StoreUpdateStatus,
  args: { kind: 'plugin', onOpenStore: fn() },
  decorators: [
    (Story) => (
      <StoreUpdatesProvider enabled>
        <Story />
      </StoreUpdatesProvider>
    ),
  ],
} satisfies Meta<typeof StoreUpdateStatus>;
export default meta;
type Story = StoryObj<typeof meta>;

export const UpdatesAvailable: Story = {
  beforeEach: api({
    check_updates: true,
    catalogs: [{ name: 'official', ok: true, checked_at: checkedAt }],
    updates: [
      {
        catalog: 'official',
        kind: 'plugin',
        id: 'official-charts',
        name: 'Charts',
        installed_version: '0.1.0',
        latest_version: '0.2.0',
      },
      {
        catalog: 'official',
        kind: 'theme',
        id: 'roastery',
        name: 'Roastery',
        installed_version: '1.0.0',
        latest_version: '1.1.0',
      },
    ],
  }),
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    // Only the updates of this kind
    await expect(await canvas.findByText('1 update available')).toBeVisible();
    await userEvent.click(canvas.getByRole('button', { name: 'Open the Store tab' }));
    await expect(args.onOpenStore).toHaveBeenCalled();

    // "Check now" asks again
    const before = calls;
    await userEvent.click(canvas.getByRole('button', { name: 'Check now' }));
    await waitFor(() => expect(calls).toBe(before + 1));
  },
};

export const UpToDate: Story = {
  beforeEach: api({
    check_updates: true,
    catalogs: [{ name: 'official', ok: true, checked_at: checkedAt }],
    updates: [],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText(/^Up to date \(checked /)).toBeVisible();
  },
};

/** A failed check is never shown as "up to date": the reason, and the time of the result shown */
export const CheckFailedShowsTheLastResult: Story = {
  beforeEach: api({
    check_updates: true,
    catalogs: [
      {
        name: 'official',
        ok: true,
        checked_at: checkedAt,
        code: 'catalog_fetch_failed',
        error: 'Fetching the catalog failed',
      },
    ],
    updates: [],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText(
        /Could not check the catalog “official”: The catalog could not be downloaded\. \(showing the result checked /
      )
    ).toBeVisible();
  },
};

export const NotCheckedYet: Story = {
  beforeEach: api({
    check_updates: false,
    catalogs: [{ name: 'official', ok: false }],
    updates: [],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Not checked for updates yet.')).toBeVisible();
    await expect(canvas.queryByText(/Up to date/)).toBeNull();
  },
};

/** With the store turned off, nothing is shown */
export const StoreDisabled: Story = {
  beforeEach: api({
    status: 404,
    body: { error: 'The package store is disabled', code: 'store_disabled' },
  }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(calls).toBe(1));
    await expect(canvasElement.querySelector('[role="status"]')).toBeNull();
  },
};
