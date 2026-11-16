import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { QueriesResponse } from '../types';
import { QueryManager } from './QueryManager';

const SAMPLE_RESPONSE: QueriesResponse = {
  timezone: 'Asia/Tokyo',
  total: 2,
  queries: [
    {
      name: 'by_brand',
      from: 'product',
      mode: 'aggregate',
      where: {},
      group_by: [{ field: 'brand' }],
      select: [
        { fn: 'count', as: 'n' },
        { fn: 'sum', field: 'price', as: 'total' },
      ],
      order_by: [{ field: 'total', dir: 'desc' }],
      limit: null,
      rows: [
        { brand: 'Aegis', n: 2, total: 4000 },
        { brand: 'Beta', n: 1, total: 500 },
      ],
    },
    {
      name: 'latest_sale',
      from: 'product',
      mode: 'list',
      where: { on_sale: { eq: true } },
      group_by: [],
      select: [],
      order_by: [{ field: 'published_at', dir: 'desc' }],
      limit: 2,
      rows: [
        {
          id: 'p1',
          content_type: 'product',
          title: 'Starter Kit',
          slug: 'starter-kit',
          excerpt: 'Complete starter bundle',
          published_at: '2026-10-01T00:00:00Z',
          created_at: '2026-10-01T00:00:00Z',
          fields: { price: 1500, on_sale: true },
          relations: {},
        },
      ],
    },
  ],
};

const queriesApi = (response: QueriesResponse) =>
  mockApi(({ method, path }) => {
    if (path === '/queries' && method === 'GET') {
      return response;
    }
    return undefined;
  });

const meta = {
  title: 'Settings/QueryManager',
  component: QueryManager,
  args: { onShowToast: fn() },
  beforeEach: () => queriesApi(SAMPLE_RESPONSE),
} satisfies Meta<typeof QueryManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('queries.by_brand')).toBeVisible();
    await expect(canvas.getByText('queries.latest_sale')).toBeVisible();
    await expect(canvas.getByText('Starter Kit')).toBeVisible();

    // Filter by mode tab
    const tabs = canvas.getAllByRole('tab');
    await expect(tabs.length).toBe(3);
    await userEvent.click(tabs[2]);
    await expect(canvas.queryByText('queries.by_brand')).not.toBeInTheDocument();
    await expect(canvas.getByText('queries.latest_sale')).toBeVisible();

    // Reset filter and search
    await userEvent.click(tabs[0]);
    const searchInput = canvas.getByRole('searchbox');
    await userEvent.type(searchInput, 'by_brand');
    await expect(canvas.getByText('queries.by_brand')).toBeVisible();
    await expect(canvas.queryByText('queries.latest_sale')).not.toBeInTheDocument();

    // Toggle JSON view
    const jsonBtn = canvas.getByRole('button', { name: 'JSON' });
    await userEvent.click(jsonBtn);
    await expect(canvas.getByText(/"brand": "Aegis"/)).toBeVisible();
  },
};

export const Empty: Story = {
  beforeEach: () => queriesApi({ timezone: 'UTC', queries: [], total: 0 }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText(/No declarative queries defined|宣言的クエリは定義されていません/)
    ).toBeVisible();
  },
};
