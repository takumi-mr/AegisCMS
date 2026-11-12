import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo, UserAccount } from '../types';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { UserManager } from './UserManager';

const user = (username: string, role: string, extra: Partial<UserAccount> = {}): UserAccount => ({
  id: `usr_${username}`,
  username,
  email: `${username}@example.com`,
  display_name: null,
  role,
  disabled: false,
  avatar_url: null,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
  ...extra,
});

const handler: MockHandler = (req) =>
  req.path === '/users'
    ? {
        users: [
          user('ada', 'admin', { display_name: 'Ada Lovelace', totp_enabled: true }),
          user('grace', 'editor'),
          user('linus', 'viewer', { disabled: true }),
          user('newcomer', 'viewer', { disabled: true, pending_approval: true }),
        ],
      }
    : undefined;

const session: SessionInfo = {
  id: 'ses_story',
  user_id: 'ada',
  role: 'admin',
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
};
const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Users/UserManager',
  component: UserManager,
  args: { session, onSelfUpdated: fn(), onShowToast: fn() },
} satisfies Meta<typeof UserManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Editor: Story = { args: { session: { ...session, role: 'editor' } } };
