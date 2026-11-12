import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import type { SessionInfo, SiteSettings as SiteSettingsData, ThemeInfo } from '../types';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import { SettingsView } from './SettingsView';

const handler: MockHandler = (req) => {
  if (req.path === '/site') {
    return {
      title: null,
      favicon: null,
      defaults: { title: 'My Aegis Site', favicon: null },
    } satisfies SiteSettingsData;
  }
  if (req.path === '/tags') {
    const tags = [
      { id: 't1', slug: 'news', name: 'News', description: null, usage: 4 },
      { id: 't2', slug: 'guides', name: 'Guides', description: 'How-to articles', usage: 2 },
    ].map((t) => ({
      ...t,
      created_at: '2026-10-01T00:00:00Z',
      updated_at: '2026-10-01T00:00:00Z',
    }));
    return { tags, total: tags.length };
  }
  if (req.path === '/themes') {
    return [
      {
        id: 'default',
        name: 'Default',
        version: '1.0.0',
        description: 'The built-in theme',
        author: 'Aegis',
        tags: [],
        is_active: true,
        has_templates: true,
        has_assets: true,
      },
    ] satisfies ThemeInfo[];
  }
  return undefined;
};

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
  title: 'Settings/SettingsView',
  component: SettingsView,
  args: {
    session,
    section: 'site',
    onSectionChange: fn(),
    categories: [],
    onCategoryCreated: fn(),
    tagsEnabled: true,
    onSelfUpdated: fn(),
    onShowToast: fn(),
  },
} satisfies Meta<typeof SettingsView>;
export default meta;
type Story = StoryObj<typeof meta>;
export const SiteSettings: Story = {};
export const Categories: Story = { args: { section: 'categories' } };
export const Tags: Story = { args: { section: 'tags' } };
export const Licenses: Story = { args: { section: 'licenses' } };
export const ViewerSettings: Story = { args: { session: { ...session, role: 'viewer' } } };
