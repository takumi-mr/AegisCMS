import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { ContentItem } from '../types';
import { CategoryManager } from './CategoryManager';

const categories: ContentItem[] = [
  {
    id: 'cat_1',
    content_type: 'category',
    title: 'Engineering',
    slug: 'engineering',
    status: 'published',
    blocks: [],
    rendered_html: '',
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z',
  },
];
const meta = {
  title: 'Content/CategoryManager',
  component: CategoryManager,
  args: { categories, canCreate: true, onCreated: fn() },
} satisfies Meta<typeof CategoryManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const WithCategories: Story = {};
export const ReadOnly: Story = { args: { canCreate: false } };
export const Empty: Story = { args: { categories: [] } };
