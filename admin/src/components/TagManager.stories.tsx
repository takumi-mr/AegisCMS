import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { TagManager } from './TagManager';

const handler: MockHandler = (req) => {
  if (req.path !== '/tags') return undefined;
  const tags = [
    { id: 't1', slug: 'news', name: 'News', description: null, usage: 4 },
    { id: 't2', slug: 'guides', name: 'Guides', description: 'How-to articles', usage: 2 },
    { id: 't3', slug: 'releases', name: 'Releases', description: null, usage: 0 },
  ].map((t) => ({ ...t, created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z' }));
  return { tags, total: tags.length };
};

// The list comes from mocked API responses (see `handler`)
const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Content/TagManager',
  component: TagManager,
  args: { canEdit: true, onShowToast: fn() },
} satisfies Meta<typeof TagManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Editable: Story = {};
export const ReadOnly: Story = { args: { canEdit: false } };
