import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { NotificationStatus } from '../types';
import { NotificationManager } from './NotificationManager';

const status: NotificationStatus = {
  enabled: true,
  channels: ['email', 'log'],
  locale: 'en',
  html: true,
  persistent: true,
  max_attempts: 5,
  staff_recipients: true,
  queue: { pending: 2, failed: 1 },
  kinds: [
    { kind: 'order.created', person: true, staff: true, disabled: false },
    { kind: 'account.verify', person: true, staff: false, disabled: false },
    { kind: 'comment.created', person: false, staff: true, disabled: true },
  ],
};

const handler: MockHandler = (req) => (req.path === '/notifications' ? status : undefined);

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Settings/NotificationManager',
  component: NotificationManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof NotificationManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
