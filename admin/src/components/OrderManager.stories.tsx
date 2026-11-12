import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { Order, OrderPage, OrderStatus, SalesReport } from '../types';
import { OrderManager } from './OrderManager';

const order = (id: string, number: string, status: OrderStatus, total: number): Order => ({
  id,
  number,
  status,
  currency: 'jpy',
  lines: [
    {
      product_id: 'p1',
      slug: 'tote-bag',
      title: 'Tote bag',
      sku: 'TOTE-01',
      unit_price: total,
      quantity: 1,
      line_total: total,
    },
  ],
  adjustments: [],
  subtotal: total,
  total,
  customer_name: 'Hanako Yamada',
  customer_email: 'hanako@example.com',
  shipping_address: null,
  coupon_code: null,
  payment_provider: 'manual',
  payment_ref: null,
  transaction_id: null,
  refunded_amount: 0,
  refunds: [],
  payment_instructions: null,
  expires_at: null,
  metadata: {},
  note: null,
  created_at: '2026-11-10T09:00:00Z',
  updated_at: '2026-11-10T09:00:00Z',
  paid_at: status === 'pending_payment' ? null : '2026-11-10T09:30:00Z',
  can_mark_paid: status === 'pending_payment',
  can_refund: status === 'paid',
  supports_online_refund: false,
});

const handler: MockHandler = (req) => {
  if (req.path === '/orders/report') {
    return {
      start: '2026-10-14',
      end: '2026-11-12',
      days: [
        { day: '2026-11-09', currency: 'jpy', orders: 2, gross: 7800, refunded: 0 },
        { day: '2026-11-10', currency: 'jpy', orders: 1, gross: 3400, refunded: 1200 },
      ],
    } satisfies SalesReport;
  }
  if (req.path === '/orders') {
    const orders = [
      order('o1', 'AE-1003', 'pending_payment', 3400),
      order('o2', 'AE-1002', 'paid', 4400),
      order('o3', 'AE-1001', 'shipped', 3400),
    ];
    return { orders, total: orders.length, limit: 25, offset: 0 } satisfies OrderPage;
  }
  return undefined;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Commerce/OrderManager',
  component: OrderManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof OrderManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
