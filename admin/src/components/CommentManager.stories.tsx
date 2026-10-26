import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { CommentManager } from './CommentManager';

const meta = {
  title: 'Moderation/CommentManager',
  component: CommentManager,
  args: { isAdmin: true, onShowToast: fn() },
} satisfies Meta<typeof CommentManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Editor: Story = { args: { isAdmin: false } };
