import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo } from '../types';
import { ThemeManager } from './ThemeManager';

const session: SessionInfo = {
  id: 'ses_story',
  user_id: 'ada',
  role: 'admin',
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
};
const meta = {
  title: 'Settings/ThemeManager',
  component: ThemeManager,
  args: { session, onShowToast: fn() },
} satisfies Meta<typeof ThemeManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Viewer: Story = { args: { session: { ...session, role: 'viewer' } } };
