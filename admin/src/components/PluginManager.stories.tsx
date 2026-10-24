import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { PluginInfo, PluginManifest, SessionInfo } from '../types';
import { PluginManager } from './PluginManager';

const limits = { max_memory_mb: 16, max_fuel_units: 100_000_000, execution_timeout_ms: 250 };

const manifest = (overrides: Partial<PluginManifest> & { name: string }): PluginManifest => ({
  version: '0.1.0',
  capabilities: { hooks: [] },
  resource_limits: limits,
  ...overrides,
});

/** The kinds of entries `GET /api/v1/plugins` returns */
const PLUGINS: PluginInfo[] = [
  {
    name: 'official-shop-rules',
    is_enabled: true,
    is_installed: true,
    manifest: manifest({
      name: 'official-shop-rules',
      display_name: 'Shop Rules (Shipping, Coupons & Tax)',
      description: 'Adds shipping fees, coupon codes and an optional tax line to shop orders.',
      author: { name: 'AegisCMS', url: 'https://example.com' },
      license: 'MIT',
      capabilities: { hooks: ['calculate-order'] },
    }),
    settings: { shipping_fee: '650', coupons: 'WELCOME10:10%' },
  },
  {
    name: 'official-code-enhancer',
    is_enabled: false,
    is_installed: true,
    manifest: manifest({
      name: 'official-code-enhancer',
      description: 'Syntax highlighting, external themes and Mermaid diagrams.',
      capabilities: {
        hooks: ['transform-content-ast', 'on-render-page'],
        network: { allowed_domains: ['cdn.jsdelivr.net', 'unpkg.com'] },
        storage: { kv_namespace: 'aegis_code_enhancer' },
      },
    }),
  },
  {
    name: 'broken-plugin',
    is_enabled: false,
    is_installed: true,
    manifest: null,
    problem: 'Failed to parse manifest: missing field `version` at line 4 column 1',
  },
  {
    name: 'removed-plugin',
    is_enabled: true,
    is_installed: false,
    manifest: null,
    problem: 'Not installed (plugins/removed-plugin/plugin.manifest.json not found)',
  },
];

const session = (role: string): SessionInfo => ({
  id: 'ses_story',
  user_id: `${role}-user`,
  role,
  created_at: '2026-10-01T00:00:00Z',
  expires_at: '2026-10-02T00:00:00Z',
  metadata: {},
});

/** Serves the plugin endpoints from an in-memory list, like the server does */
const pluginApi = (initial: PluginInfo[]) => {
  let plugins = structuredClone(initial);
  return mockApi(({ method, path, body }) => {
    if (method === 'GET' && path === '/plugins') return plugins;

    const enabled = path.match(/^\/plugins\/([^/]+)\/enabled$/);
    if (method === 'PUT' && enabled) {
      const name = decodeURIComponent(enabled[1]);
      const on = (body as { enabled: boolean }).enabled;
      plugins = plugins.map((p) => (p.name === name ? { ...p, is_enabled: on } : p));
      return plugins;
    }

    const settings = path.match(/^\/plugins\/([^/]+)\/settings$/);
    if (method === 'PUT' && settings) {
      const name = decodeURIComponent(settings[1]);
      const next = (body as { settings: Record<string, string> }).settings;
      plugins = plugins.map((p) => (p.name === name ? { ...p, settings: next } : p));
      return { name, settings: next };
    }
    return undefined;
  });
};

const meta = {
  title: 'Settings/PluginManager',
  component: PluginManager,
  args: { session: session('admin'), onShowToast: fn() },
  beforeEach: () => pluginApi(PLUGINS),
} satisfies Meta<typeof PluginManager>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Enabled, disabled, broken and enabled-but-removed plugins */
export const Default: Story = {};

export const Empty: Story = {
  beforeEach: () => pluginApi([]),
};

/** Without the admin role the enable / disable buttons are off */
export const NotAdmin: Story = {
  args: { session: session('editor') },
};

/** The server refuses the switch (e.g. a plugin used as a notification channel) */
export const SwitchRefused: Story = {
  beforeEach: () =>
    mockApi(({ method, path }) => {
      if (method === 'GET' && path === '/plugins') return PLUGINS;
      if (method === 'PUT' && path.endsWith('/enabled')) {
        return {
          status: 409,
          body: { error: "Plugin 'official-shop-rules' is a notification channel" },
        };
      }
      return undefined;
    }),
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const card = await canvas.findByRole('article', {
      name: 'Shop Rules (Shipping, Coupons & Tax)',
    });
    window.confirm = () => true;
    await userEvent.click(within(card).getByRole('button', { name: 'Disable' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        "Plugin 'official-shop-rules' is a notification channel",
        'error'
      )
    );
    // The card keeps its state
    await expect(within(card).getByText('Enabled', { exact: true })).toBeVisible();
  },
};

/** Enables a disabled plugin from its card */
export const EnablePlugin: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const card = await canvas.findByRole('article', { name: 'official-code-enhancer' });
    await userEvent.click(within(card).getByRole('button', { name: 'Enable' }));
    await expect(await within(card).findByText('Enabled', { exact: true })).toBeVisible();
    await expect(args.onShowToast).toHaveBeenCalledWith(
      'Enabled plugin "official-code-enhancer"',
      'success'
    );
  },
};
