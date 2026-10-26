import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo } from '../types';
import { SettingsView } from './SettingsView';

const session: SessionInfo = {
  id: 'ses_story',
  user_id: 'ada',
  role: 'admin',
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
};
const meta = {
  title: 'Settings/SettingsView',
  component: SettingsView,
  args: {
    session,
    section: 'site',
    onSectionChange: fn(),
    categories: [],
    onCategoryCreated: fn(),
    onSelfUpdated: fn(),
    onShowToast: fn(),
  },
} satisfies Meta<typeof SettingsView>;
export default meta;
type Story = StoryObj<typeof meta>;
export const SiteSettings: Story = {};
export const Categories: Story = { args: { section: 'categories' } };
export const ViewerSettings: Story = { args: { session: { ...session, role: 'viewer' } } };
