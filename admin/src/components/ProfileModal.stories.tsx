import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { UserAccount } from '../types';
import { ProfileModal } from './ProfileModal';

const user: UserAccount = {
  id: 'usr_01',
  username: 'ada',
  email: 'ada@example.com',
  display_name: 'Ada Lovelace',
  role: 'admin',
  disabled: false,
  avatar_url: null,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
};
const meta = {
  title: 'Users/ProfileModal',
  component: ProfileModal,
  args: { user, canChangePassword: true, onUpdated: fn(), onShowToast: fn(), onClose: fn() },
} satisfies Meta<typeof ProfileModal>;
export default meta;
type Story = StoryObj<typeof meta>;
export const CanChangePassword: Story = {};
export const SingleSignOn: Story = { args: { canChangePassword: false } };
