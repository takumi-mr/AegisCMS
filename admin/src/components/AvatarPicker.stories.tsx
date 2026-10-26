import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { UserAccount } from '../types';
import { AvatarPicker } from './AvatarPicker';

const user: UserAccount = {
  id: 'usr_01',
  username: 'ada',
  email: 'ada@example.com',
  display_name: 'Ada Lovelace',
  role: 'admin',
  disabled: false,
  avatar_url: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const meta = {
  title: 'Users/AvatarPicker',
  component: AvatarPicker,
  args: { user, onChanged: fn() },
} satisfies Meta<typeof AvatarPicker>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Initials: Story = {};
export const WithAvatar: Story = { args: { user: { ...user, avatar_url: '/uploads/ada.png' } } };
