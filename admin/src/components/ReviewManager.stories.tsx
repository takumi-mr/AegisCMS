import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { ReviewManager } from './ReviewManager';

const meta = {
  title: 'Shop/ReviewManager',
  component: ReviewManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof ReviewManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
