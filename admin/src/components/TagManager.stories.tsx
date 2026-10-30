import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { TagManager } from './TagManager';

// The list is loaded from the server; without one the page shows its error notice above the form
const meta = {
  title: 'Content/TagManager',
  component: TagManager,
  args: { canEdit: true, onShowToast: fn() },
} satisfies Meta<typeof TagManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Editable: Story = {};
export const ReadOnly: Story = { args: { canEdit: false } };
