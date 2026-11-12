import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { ScheduleItem, ScheduleResponse } from '../types';
import { ScheduleCalendar } from './ScheduleCalendar';

const handler: MockHandler = (req) => {
  if (req.path !== '/schedule') return undefined;
  const items: ScheduleItem[] = [
    {
      id: 'a1',
      content_type: 'article',
      title: 'Autumn campaign',
      slug: 'autumn-campaign',
      status: 'scheduled',
      published_at: '2026-11-15T09:00:00Z',
      unpublish_at: null,
      author_id: 'ada',
      updated_at: '2026-11-10T09:00:00Z',
    },
    {
      id: 'a2',
      content_type: 'article',
      title: 'Hello world',
      slug: 'hello-world',
      status: 'published',
      published_at: '2026-11-05T09:00:00Z',
      unpublish_at: '2026-11-25T09:00:00Z',
      author_id: 'ada',
      updated_at: '2026-11-05T09:00:00Z',
    },
  ];
  return { items, total: items.length } satisfies ScheduleResponse;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Content/ScheduleCalendar',
  component: ScheduleCalendar,
  args: { onOpenContent: fn(), onShowToast: fn() },
} satisfies Meta<typeof ScheduleCalendar>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Calendar: Story = {};
