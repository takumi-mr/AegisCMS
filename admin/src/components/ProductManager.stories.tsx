import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { ProductManager } from './ProductManager';

const meta = {
  title: 'Commerce/ProductManager',
  component: ProductManager,
  args: { categories: [], onShowToast: fn() },
} satisfies Meta<typeof ProductManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const EmptyCatalog: Story = {};
