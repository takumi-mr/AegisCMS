import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { SeoSettings } from '../types';
import { SeoManager } from './SeoManager';

const defaultSettings: SeoSettings = {
  robots_txt: null,
  meta_description: 'Fast, secure, headless and zero-trust CMS engine built in Rust.',
  meta_keywords: 'cms, rust, headless, secure',
  canonical_base_url: 'https://example.com',
  og_image: '/uploads/og-default.png',
  noindex_all: false,
  twitter_card: 'summary_large_image',
  defaults: {
    robots_txt: 'User-agent: *\nDisallow: /admin/\nDisallow: /api/\nAllow: /\n',
    meta_description: 'Next-Generation Secure CMS with Sandboxed Extensibility',
  },
};

const meta = {
  title: 'Settings/SeoManager',
  component: SeoManager,
  parameters: {
    layout: 'fullscreen',
  },
  args: {
    onShowToast: fn(),
  },
} satisfies Meta<typeof SeoManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    let current = { ...defaultSettings };
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/seo') {
        return current;
      }
      if (req.method === 'PUT' && req.path === '/seo') {
        current = { ...current, ...(req.body as Partial<SeoSettings>) };
        return current;
      }
      return undefined;
    });
  },
};

export const NoindexActive: Story = {
  beforeEach: () => {
    const current = {
      ...defaultSettings,
      noindex_all: true,
      robots_txt: 'User-agent: *\nDisallow: /\n',
    };
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/seo') {
        return current;
      }
      return undefined;
    });
  },
};
