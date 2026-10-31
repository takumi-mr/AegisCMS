import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { PluginInfo, PluginManifest, PluginOrder, SessionInfo } from '../types';
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
    name: 'official-blocks',
    is_enabled: false,
    is_installed: true,
    manifest: manifest({
      name: 'official-blocks',
      description: 'Button, link card and video blocks.',
      capabilities: {
        hooks: ['render-blocks'],
        embed: { frame_domains: ['www.youtube-nocookie.com', 'player.vimeo.com'] },
      },
    }),
  },
  {
    name: 'official-page-toc',
    is_enabled: false,
    is_installed: true,
    manifest: manifest({
      name: 'official-page-toc',
      display_name: 'Page TOC & Reading Progress',
      description: 'A table of contents and a reading progress bar beside articles.',
      capabilities: { hooks: ['on-render-page'] },
      settings: [
        {
          key: 'position',
          type: 'select',
          label: { en: 'Position', ja: '位置' },
          default: 'right',
          options: [
            { value: 'right', label: { en: 'Right', ja: '右' } },
            { value: 'left', label: { en: 'Left', ja: '左' } },
          ],
        },
        {
          key: 'top_offset_px',
          type: 'number',
          label: { en: 'Top offset (px)', ja: '上からの位置 (px)' },
          description: { en: 'Also the space above a heading after jumping to it.' },
          min: 0,
          max: 400,
          default: 96,
        },
        {
          key: 'show_progress',
          type: 'boolean',
          label: { en: 'Show progress bar', ja: '進捗バーを出す' },
          default: true,
        },
        {
          key: 'accent_color',
          type: 'color',
          label: { en: 'Accent color', ja: '強調色' },
          placeholder: '#6366f1',
        },
        { key: 'title', type: 'string', label: { en: 'Title', ja: '目次の見出し' } },
      ],
    }),
    // `legacy_flag` is not declared: it stays in the key/value list
    settings: { position: 'left', legacy_flag: 'on' },
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

const enabledNames = (plugins: PluginInfo[]) =>
  plugins.filter((p) => p.is_enabled).map((p) => p.name);

/** Like the server: the enabled plugins in `enabled` order (hook order), then the others */
const listed = (plugins: PluginInfo[], enabled: string[]) => [
  ...enabled.flatMap((name) => {
    const plugin = plugins.find((p) => p.name === name);
    return plugin ? [{ ...plugin, is_enabled: true }] : [];
  }),
  ...plugins.filter((p) => !enabled.includes(p.name)).map((p) => ({ ...p, is_enabled: false })),
];

/** Serves the plugin endpoints from an in-memory list, like the server does */
const pluginApi = (initial: PluginInfo[]) => {
  let plugins = structuredClone(initial);
  // `[plugins].enabled` of the configuration file, and whether Admin Studio changed the list
  const configured = enabledNames(initial);
  let customized = false;
  const order = (): PluginOrder => ({ enabled: enabledNames(plugins), configured, customized });
  return mockApi(({ method, path, body }) => {
    if (method === 'GET' && path === '/plugins') return plugins;

    if (path === '/plugins/order') {
      if (method === 'PUT') {
        plugins = listed(plugins, (body as { enabled: string[] }).enabled);
        customized = true;
      } else if (method === 'DELETE') {
        plugins = listed(plugins, configured);
        customized = false;
      }
      return order();
    }

    const enabled = path.match(/^\/plugins\/([^/]+)\/enabled$/);
    if (method === 'PUT' && enabled) {
      const name = decodeURIComponent(enabled[1]);
      const on = (body as { enabled: boolean }).enabled;
      const current = enabledNames(plugins).filter((n) => n !== name);
      plugins = listed(plugins, on ? [...current, name] : current);
      customized = true;
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
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 埋め込みを許可したホストが権限の欄に出ること
    await expect(
      await canvas.findByText(/www\.youtube-nocookie\.com, player\.vimeo\.com/)
    ).toBeVisible();
  },
};

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
      if (method === 'GET' && path === '/plugins/order') {
        const enabled = enabledNames(PLUGINS);
        return { enabled, configured: enabled, customized: false } satisfies PluginOrder;
      }
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

/** Moves a plugin down the hook order, then goes back to the configuration file's list */
export const Reorder: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const panel = await canvas.findByRole('region', { name: 'Hook order' });
    await expect(
      within(panel).getByText('Same as [plugins].enabled of the configuration file')
    ).toBeVisible();

    await userEvent.click(
      within(panel).getByRole('button', { name: 'Move "official-shop-rules" down' })
    );
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Changed the hook order', 'success')
    );
    const items = within(panel).getAllByRole('listitem');
    await expect(items[0]).toHaveTextContent('removed-plugin');
    await expect(items[1]).toHaveTextContent('Shop Rules');

    const win = canvasElement.ownerDocument.defaultView ?? window;
    win.confirm = () => true;
    window.confirm = () => true;
    const resetBtn = await within(panel).findByRole('button', {
      name: 'Use the configuration file',
    });
    await waitFor(() => expect(resetBtn).not.toBeDisabled());
    await userEvent.click(resetBtn);
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        'Using the plugins of the configuration file',
        'success'
      )
    );
    await expect(within(panel).getAllByRole('listitem')[0]).toHaveTextContent('Shop Rules');
    await expect(
      within(panel).queryByRole('button', { name: 'Use the configuration file' })
    ).not.toBeInTheDocument();
  },
};

/** Settings the manifest declares are form fields; the others stay in the key/value list */
export const DeclaredSettings: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const card = await canvas.findByRole('article', { name: 'Page TOC & Reading Progress' });
    await userEvent.click(within(card).getByRole('button', { name: /Settings/ }));
    await canvas.findByText('Configure Page TOC & Reading Progress');
    const dialog = canvas;

    // The saved value is selected; the default shows for the empty ones
    await expect(dialog.getByLabelText(/Position/)).toHaveValue('left');
    await expect(dialog.getByLabelText(/Top offset/)).toHaveAttribute('placeholder', '96');
    await expect(dialog.getByLabelText(/Show progress bar/)).toHaveDisplayValue('Default (On)');
    // Undeclared keys are listed apart
    await expect(dialog.getByText('Other settings')).toBeInTheDocument();
    await expect(dialog.getByDisplayValue('on')).toBeInTheDocument();

    // An out-of-range number is refused before it is sent
    const offset = dialog.getByLabelText(/Top offset/);
    await userEvent.type(offset, '9999');
    await expect(await dialog.findByText('Must be 400 or less')).toBeInTheDocument();
    await userEvent.click(dialog.getByRole('button', { name: 'Save Settings' }));
    await expect(args.onShowToast).toHaveBeenCalledWith(
      'Fix the highlighted settings first',
      'error'
    );

    // A valid value is saved, and an emptied field is left out ("not set")
    await userEvent.clear(offset);
    await userEvent.type(offset, '64');
    await userEvent.selectOptions(dialog.getByLabelText(/Position/), 'right');
    await userEvent.selectOptions(dialog.getByLabelText(/Position/), '');
    await userEvent.type(dialog.getByLabelText(/Accent color/), '#e11d48');
    await userEvent.click(dialog.getByRole('button', { name: 'Save Settings' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        "Settings for plugin 'official-page-toc' saved",
        'success'
      )
    );
    // legacy_flag and the three fields filled in (position and title are not set)
    await expect(await within(card).findByText('3 keys')).toBeVisible();
  },
};
