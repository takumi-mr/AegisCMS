import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { NotificationManager } from './NotificationManager';

const meta = {
  title: 'Settings/NotificationManager',
  component: NotificationManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof NotificationManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
