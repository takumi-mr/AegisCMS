import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { AnalyticsSummary } from '../types';
import { lastDays } from '../utils/analytics';
import { AnalyticsView } from './AnalyticsView';

/** A deterministic month of visits ending today (weekly rhythm and a launch spike) */
const sampleSummary = (days: number): AnalyticsSummary => {
  const { from, to } = lastDays(days);
  const start = new Date(`${from}T00:00:00Z`);
  const daily = Array.from({ length: days }, (_, i) => {
    const day = new Date(start.getTime() + i * 86_400_000).toISOString().slice(0, 10);
    const weekday = new Date(`${day}T00:00:00Z`).getUTCDay();
    const base = weekday === 0 || weekday === 6 ? 60 : 110;
    const spike = i === days - 6 ? 240 : 0;
    const visitors = base + ((i * 37) % 23) + Math.round(spike / 3);
    return { day, pageviews: Math.round(visitors * 2.4) + spike, visitors };
  });
  return {
    from,
    to,
    pageviews: daily.reduce((n, d) => n + d.pageviews, 0),
    visitors: daily.reduce((n, d) => n + d.visitors, 0),
    daily,
    top_pages: [
      { value: '/', pageviews: 2140, visitors: 1290 },
      { value: '/docs/getting-started', pageviews: 980, visitors: 640 },
      { value: '/articles/website-launch', pageviews: 610, visitors: 520 },
      { value: '/pricing', pageviews: 240, visitors: 190 },
    ],
    top_referrers: [
      { value: '', pageviews: 820, visitors: 820 },
      { value: 'google.com', pageviews: 540, visitors: 540 },
      { value: 'github.com', pageviews: 310, visitors: 310 },
      { value: 'news.ycombinator.com', pageviews: 120, visitors: 120 },
    ],
    devices: [
      { value: 'desktop', pageviews: 2600, visitors: 1500 },
      { value: 'mobile', pageviews: 1250, visitors: 1020 },
      { value: 'tablet', pageviews: 120, visitors: 90 },
    ],
    top_utm_sources: [
      { value: 'twitter', pageviews: 450, visitors: 320 },
      { value: 'newsletter', pageviews: 210, visitors: 180 },
    ],
    top_utm_campaigns: [{ value: 'launch2026', pageviews: 450, visitors: 320 }],
    top_utm_mediums: [{ value: 'social', pageviews: 450, visitors: 320 }],
    previous: {
      from: '2026-09-01',
      to: '2026-09-30',
      pageviews: 3200,
      visitors: 1800,
      pageviews_change_pct: 24.1,
      visitors_change_pct: 18.5,
    },
  };
};

const meta = {
  title: 'Analytics/AnalyticsView',
  component: AnalyticsView,
  parameters: { layout: 'fullscreen' },
  args: { onShowToast: fn() },
} satisfies Meta<typeof AnalyticsView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    let requests = 0;
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/analytics/summary') {
        requests += 1;
        // The first request is the default 30 days; later ones (the 7-day tab) answer a week
        return sampleSummary(requests === 1 ? 30 : 7);
      }
      return undefined;
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Top pages')).toBeVisible();
    await expect(canvas.getByRole('button', { name: 'Export CSV' })).toBeVisible();
    await expect(canvas.getByText('Campaign sources (utm_source)')).toBeVisible();
    await expect(canvas.getByText('/docs/getting-started')).toBeVisible();
    await expect(canvas.getByText('Direct / none')).toBeVisible();
    await expect(canvas.getByText('Mobile')).toBeVisible();

    // Arrow keys on the chart read each day in the tooltip
    const chart = canvas.getByRole('img', { name: /Daily pageviews and visitors/ });
    chart.focus();
    await userEvent.keyboard('{ArrowLeft}');
    await expect(await canvas.findByRole('status')).toHaveTextContent('Pageviews');

    // The same numbers as a table
    await userEvent.click(canvas.getByRole('button', { name: 'Show as table' }));
    await expect(canvas.getByRole('columnheader', { name: 'Date' })).toBeVisible();
    await userEvent.click(canvas.getByRole('button', { name: 'Show as chart' }));

    await userEvent.click(canvas.getByRole('button', { name: 'Last 7 days' }));
    await waitFor(() =>
      expect(canvas.getByRole('button', { name: 'Last 7 days' })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
    );
    await userEvent.click(canvas.getByRole('button', { name: 'Show as table' }));
    // One row per day of the week
    await waitFor(() => {
      const table = canvas.getByRole('columnheader', { name: 'Date' }).closest('table');
      return expect(table?.querySelectorAll('tbody tr')).toHaveLength(7);
    });
  },
};

export const NoVisitsYet: Story = {
  beforeEach: () =>
    mockApi((req) => {
      if (req.path !== '/analytics/summary') return undefined;
      const summary = sampleSummary(30);
      return {
        ...summary,
        pageviews: 0,
        visitors: 0,
        daily: summary.daily.map((d) => ({ ...d, pageviews: 0, visitors: 0 })),
        top_pages: [],
        top_referrers: [],
        devices: [],
        top_utm_sources: [],
        top_utm_campaigns: [],
        top_utm_mediums: [],
        previous: null,
      };
    }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('No visits recorded in this period yet.')).toBeVisible();
    await expect(canvas.getAllByText('No data')).toHaveLength(5);
  },
};

export const LoadError: Story = {
  beforeEach: () =>
    mockApi((req) =>
      req.path === '/analytics/summary'
        ? { status: 500, body: { error: 'Could not read analytics' } }
        : undefined
    ),
  play: async ({ args }) => {
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Could not read analytics', 'error')
    );
  },
};
