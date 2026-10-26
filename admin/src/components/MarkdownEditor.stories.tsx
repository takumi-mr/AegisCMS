import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { MarkdownEditor } from './MarkdownEditor';

const meta = {
  title: 'Content/MarkdownEditor',
  component: MarkdownEditor,
  args: {
    value: '## Welcome\n\nWrite **Markdown** here.',
    blocks: [],
    onChange: fn(),
    onSave: fn(),
  },
} satisfies Meta<typeof MarkdownEditor>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Article: Story = {};
export const Empty: Story = { args: { value: '', blocks: [] } };
