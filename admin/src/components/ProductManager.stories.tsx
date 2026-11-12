import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { ShopConfig } from '../types';
import { ProductManager } from './ProductManager';

const handler: MockHandler = (req) =>
  req.path === '/shop/config'
    ? ({
        enabled: true,
        members_enabled: true,
        reviews_enabled: true,
        currency: 'jpy',
        product_type: 'product',
        provider: 'manual',
      } satisfies ShopConfig)
    : undefined;

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Commerce/ProductManager',
  component: ProductManager,
  args: { categories: [], onShowToast: fn() },
} satisfies Meta<typeof ProductManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const EmptyCatalog: Story = {};
