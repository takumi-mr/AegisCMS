import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { BrokenLinksReport } from '../types';
import { BrokenLinksManager } from './BrokenLinksManager';

const REPORT_WITH_ITEMS: BrokenLinksReport = {
  last_scanned_at: '2026-10-31T12:00:00Z',
  total_links_checked: 15,
  broken_count: 2,
  items: [
    {
      id: 'item_1',
      content_type: 'article',
      content_id: 'art_1',
      content_title: 'Getting Started with AegisCMS',
      content_slug: 'getting-started',
      target_url: 'https://example.com/dead-link',
      is_external: true,
      status_code: 404,
      error_message: 'HTTP 404 Not Found',
    },
    {
      id: 'item_2',
      content_type: 'page',
      content_id: 'page_1',
      content_title: 'About Us',
      content_slug: 'about',
      target_url: '/missing-page',
      is_external: false,
      status_code: 404,
      error_message: 'Target content does not exist',
    },
  ],
};

const brokenLinksApi = (initial: { isScanning: boolean; report: BrokenLinksReport }) => {
  const isScanning = initial.isScanning;
  const report = structuredClone(initial.report);

  return mockApi(({ method, path }) => {
    if (path === '/broken-links' && method === 'GET') {
      return { is_scanning: isScanning, report };
    }
    if (path === '/broken-links/scan' && method === 'POST') {
      return { status: 202, body: { message: 'Scan started' } };
    }
    return undefined;
  });
};

const meta = {
  title: 'Settings/BrokenLinksManager',
  component: BrokenLinksManager,
  args: { onShowToast: fn() },
  beforeEach: () =>
    brokenLinksApi({
      isScanning: false,
      report: REPORT_WITH_ITEMS,
    }),
} satisfies Meta<typeof BrokenLinksManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('https://example.com/dead-link')).toBeInTheDocument();
    await expect(canvas.getByText('/missing-page')).toBeInTheDocument();
    await expect(canvas.getByText('Getting Started with AegisCMS')).toBeInTheDocument();
    await expect(canvas.getByText('About Us')).toBeInTheDocument();
  },
};

export const FilterSearch: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('https://example.com/dead-link')).toBeInTheDocument();
    const searchInput = canvas.getByPlaceholderText('Search by URL or title...');
    await userEvent.type(searchInput, 'About Us');
    await expect(canvas.getByText('About Us')).toBeInTheDocument();
    await expect(canvas.queryByText('Getting Started with AegisCMS')).not.toBeInTheDocument();
  },
};

export const CleanState: Story = {
  beforeEach: () =>
    brokenLinksApi({
      isScanning: false,
      report: {
        last_scanned_at: '2026-10-31T12:00:00Z',
        total_links_checked: 10,
        broken_count: 0,
        items: [],
      },
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText('No broken links found. All checked links are valid.')
    ).toBeInTheDocument();
  },
};

export const NeverScanned: Story = {
  beforeEach: () =>
    brokenLinksApi({
      isScanning: false,
      report: {
        last_scanned_at: null,
        total_links_checked: 0,
        broken_count: 0,
        items: [],
      },
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText(
        'No scan has been performed yet. Click "Scan Now" to check for broken links.'
      )
    ).toBeInTheDocument();
  },
};
