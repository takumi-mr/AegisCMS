import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { AdminMember, MemberPage } from '../types';
import { MemberManager } from './MemberManager';

const member = (id: string, name: string, level: string): AdminMember => ({
  id,
  email: `${name.toLowerCase()}@example.com`,
  name,
  nickname: null,
  avatar_url: null,
  phone: null,
  status: 'active',
  level,
  shipping_address: null,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
  last_login_at: '2026-11-11T08:00:00Z',
});

const handler: MockHandler = (req) => {
  if (req.path !== '/admin/members') return undefined;
  const members = [member('m1', 'Hanako', 'paid'), member('m2', 'Taro', 'free')];
  return { members, total: members.length } satisfies MemberPage;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Shop/MemberManager',
  component: MemberManager,
  args: { onShowToast: fn() },
} satisfies Meta<typeof MemberManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
