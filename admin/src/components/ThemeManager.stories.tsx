import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo, ThemeInfo } from '../types';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { ThemeManager } from './ThemeManager';

const theme = (id: string, name: string, active: boolean): ThemeInfo => ({
  id,
  name,
  version: '1.0.0',
  description: `${name} theme`,
  author: { name: 'Aegis Community' },
  tags: ['blog'],
  is_active: active,
  has_templates: true,
  has_assets: true,
});

const handler: MockHandler = (req) =>
  req.path === '/themes'
    ? [theme('default', 'Default', true), theme('minimal', 'Minimal', false)]
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
  title: 'Settings/ThemeManager',
  component: ThemeManager,
  args: { session, onShowToast: fn() },
} satisfies Meta<typeof ThemeManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Viewer: Story = { args: { session: { ...session, role: 'viewer' } } };
