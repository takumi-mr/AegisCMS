import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { OrderManager } from './OrderManager';

const meta = {
  title: 'Commerce/OrderManager',
  component: OrderManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof OrderManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
