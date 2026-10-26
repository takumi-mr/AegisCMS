import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo } from '../types';
import { UserManager } from './UserManager';

const session: SessionInfo = {
  id: 'ses_story',
  user_id: 'ada',
  role: 'admin',
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
};
const meta = {
  title: 'Users/UserManager',
  component: UserManager,
  args: { session, onSelfUpdated: fn(), onShowToast: fn() },
} satisfies Meta<typeof UserManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Editor: Story = { args: { session: { ...session, role: 'editor' } } };
