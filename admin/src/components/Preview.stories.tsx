import type { Meta, StoryObj } from '@storybook/react-vite';
import { Preview } from './Preview';

const meta = {
  title: 'Content/Preview',
  component: Preview,
  args: {
    contentType: 'article',
    title: 'A sample article',
    slug: 'sample-article',
    categoryId: '',
    blocks: [
      { name: 'core/heading', attributes: { level: 2 }, text: 'Preview heading' },
      { name: 'core/paragraph', attributes: {}, text: 'Preview content appears here.' },
    ],
  },
} satisfies Meta<typeof Preview>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Article: Story = {};
export const BlankPage: Story = {
  args: { contentType: 'page', layout: 'blank', title: 'Landing page' },
};
