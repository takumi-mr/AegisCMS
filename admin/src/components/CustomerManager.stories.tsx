import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { CustomerManager } from './CustomerManager';

const meta = {
  title: 'Shop/CustomerManager',
  component: CustomerManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof CustomerManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
