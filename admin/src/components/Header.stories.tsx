import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo } from '../types';
import { Header } from './Header';

const session: SessionInfo = {
  id: 'ses_story',
  user_id: 'ada',
  role: 'admin',
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
};
const meta = {
  title: 'Navigation/Header',
  component: Header,
  args: {
    view: 'editor',
    onViewChange: fn(),
    isOnline: true,
    onSave: fn(),
    onPublish: fn(),
    isSaving: false,
    session,
    canSave: true,
    canPublish: true,
    showShop: true,
    showComments: true,
    showForum: true,
    profile: null,
    onOpenProfile: fn(),
    onLogout: fn(),
  },
} satisfies Meta<typeof Header>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Online: Story = {};
export const Offline: Story = { args: { isOnline: false } };
export const Saving: Story = { args: { isSaving: true } };
