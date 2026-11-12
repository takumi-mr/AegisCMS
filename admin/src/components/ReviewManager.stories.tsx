import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { AdminReview, ReviewPage } from '../types';
import { ReviewManager } from './ReviewManager';

const review = (id: string, rating: number, title: string, body: string): AdminReview => ({
  id,
  product_id: 'p1',
  order_id: 'o1',
  member_id: 'm1',
  author_name: 'Hanako',
  author_email: 'hanako@example.com',
  rating,
  title,
  body,
  status: 'pending',
  verified_purchase: 1,
  reply: null,
  replied_at: null,
  rejection_reason: null,
  created_at: '2026-11-11T09:00:00Z',
  updated_at: '2026-11-11T09:00:00Z',
});

const handler: MockHandler = (req) => {
  if (req.path !== '/admin/reviews') return undefined;
  const reviews = [
    review('r1', 5, 'Great bag', 'Sturdy and the right size.'),
    review('r2', 3, 'Okay', 'The color is slightly different from the photo.'),
  ];
  return { reviews, total: reviews.length } satisfies ReviewPage;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Shop/ReviewManager',
  component: ReviewManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof ReviewManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
