import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import type { StorePackage, StorePackagesResponse, StoreVersion } from '../types';
import { mockApi, type MockRequest } from '../../.storybook/mockApi';
import { StorePanel } from './StorePanel';

const version = (v: string, extra: Partial<StoreVersion> = {}): StoreVersion => ({
  version: v,
  min_aegis_version: '0.1.0',
  capabilities: {},
  url: `https://example.com/${v}.zip`,
  sha256: '0'.repeat(64),
  size: 137418,
  public_key: 'e'.repeat(64),
  compatible: true,
  ...extra,
});

const pkg = (p: Partial<StorePackage> & Pick<StorePackage, 'id' | 'name'>): StorePackage => ({
  catalog: 'official',
  kind: 'plugin',
  description: `${p.name} description`,
  publisher: 'official',
  license: 'MIT OR Apache-2.0',
  recommended: false,
  installed_from_catalog: true,
  update_available: false,
  versions: [version('1.0.0')],
  latest_compatible: '1.0.0',
  ...p,
});

const listing: StorePackagesResponse = {
  catalogs: [
    {
      name: 'official',
      url: 'https://takumi-mr.github.io/AegisCMS-Store/catalog.json',
      ok: true,
      sequence: 12,
    },
  ],
  packages: [
    pkg({
      id: 'official-ads',
      name: 'Ad & Banner Delivery',
      versions: [
        version('0.3.1', {
          capabilities: {
            hooks: ['on-render-page', 'render-blocks'],
            csp: { script_domains: ['pagead2.googlesyndication.com'] },
          },
        }),
      ],
      latest_compatible: '0.3.1',
    }),
    pkg({
      id: 'official-charts',
      name: 'Charts',
      installed_version: '0.1.0',
      latest_compatible: '0.2.0',
      update_available: true,
      installed_capabilities: { hooks: ['render-blocks'] },
      versions: [
        version('0.2.0', {
          capabilities: {
            hooks: ['render-blocks', 'on-render-page'],
            csp: { img_domains: ['charts.example.com'] },
          },
        }),
        version('0.1.0', { capabilities: { hooks: ['render-blocks'] } }),
      ],
    }),
    pkg({
      id: 'official-forked',
      name: 'Forked Plugin',
      installed_version: '1.0.0',
      installed_from_catalog: false,
      versions: [version('2.0.0'), version('1.0.0')],
      latest_compatible: '2.0.0',
    }),
    pkg({
      id: 'official-lightbox',
      name: 'Lightbox',
      installed_version: '1.0.0',
      recommended: true,
    }),
    pkg({ id: 'official-code-enhancer', name: 'Code Highlighter', recommended: true }),
    pkg({
      id: 'official-future',
      name: 'Future Plugin',
      latest_compatible: undefined,
      versions: [version('2.0.0', { min_aegis_version: '9.0.0', compatible: false })],
    }),
    pkg({ id: 'minimal-editorial', name: 'Minimal Editorial', kind: 'theme' }),
  ],
};

let installs: MockRequest[] = [];

const storeApi =
  (response: unknown = listing) =>
  () =>
    mockApi((req) => {
      if (req.path === '/store/packages') return response;
      if (req.path === '/store/install' && req.method === 'POST') {
        installs.push(req);
        const body = req.body as { kind: string; id: string; catalog: string; version: string };
        return { ...body, installed: {} };
      }
      return undefined;
    });

const meta = {
  title: 'Settings/StorePanel',
  component: StorePanel,
  beforeEach: () => {
    installs = [];
    return storeApi()();
  },
  args: { kind: 'plugin', canInstall: true, onInstalled: fn(), onShowToast: fn() },
} satisfies Meta<typeof StorePanel>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Plugins: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Ad & Banner Delivery')).toBeVisible();
    // Only packages of this kind
    await expect(canvas.queryByText('Minimal Editorial')).toBeNull();
    await expect(canvas.getByText('v0.1.0 installed, v0.2.0 available')).toBeVisible();
    await expect(canvas.getByText('Installed (v1.0.0)')).toBeVisible();
    await expect(canvas.getByText('Needs AegisCMS 9.0.0 or later')).toBeVisible();
    // Nothing to do for an up-to-date or incompatible package
    await expect(canvas.queryByRole('button', { name: /Lightbox/ })).toBeNull();
    await expect(canvas.queryByRole('button', { name: /Future Plugin/ })).toBeNull();

    await userEvent.type(canvas.getByRole('searchbox'), 'chart');
    await expect(canvas.queryByText('Ad & Banner Delivery')).toBeNull();
    await expect(canvas.getByText('Charts')).toBeVisible();
  },
};

export const RecommendedComeFirst: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const group = await canvas.findByRole('region', { name: 'Recommended' });
    // Only the recommended package that is not installed yet
    await expect(within(group).getAllByRole('article')).toHaveLength(1);
    await expect(within(group).getByText('Code Highlighter')).toBeVisible();
    await expect(
      within(group).getByRole('button', { name: 'Install Code Highlighter' })
    ).toBeEnabled();
    // An installed recommended package stays in the full list, with its badge
    const lightbox = canvas.getByRole('article', { name: 'Lightbox' });
    await expect(within(group).queryByText('Lightbox')).toBeNull();
    await expect(within(lightbox).getByText('Recommended')).toBeVisible();
    await expect(canvas.getByRole('heading', { name: 'All packages' })).toBeVisible();
  },
};

export const Themes: Story = {
  args: { kind: 'theme' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Minimal Editorial')).toBeVisible();
    await expect(canvas.queryByText('Ad & Banner Delivery')).toBeNull();
  },
};

export const InstallShowsWhatItMayDoFirst: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Install Ad & Banner Delivery' })
    );

    const dialog = within(await within(document.body).findByRole('dialog'));
    const caps = dialog.getByTestId('store-capabilities');
    await expect(caps).toHaveTextContent('on-render-page, render-blocks');
    await expect(caps).toHaveTextContent('script-src: pagead2.googlesyndication.com');
    await expect(installs).toHaveLength(0);

    await userEvent.click(dialog.getByRole('button', { name: 'Install' }));
    await waitFor(() => expect(args.onInstalled).toHaveBeenCalled());
    await expect(installs[0].body).toEqual({
      catalog: 'official',
      kind: 'plugin',
      id: 'official-ads',
      version: '0.3.1',
    });
    await expect(args.onShowToast).toHaveBeenCalledWith(
      'Installed Ad & Banner Delivery v0.3.1',
      'success'
    );
  },
};

export const UpdateInstallsTheLatestCompatible: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Update Charts to v0.2.0' }));
    const dialog = within(await within(document.body).findByRole('dialog'));
    await expect(dialog.getByText('It replaces the installed v0.1.0.')).toBeVisible();
    // Only what the new version adds is listed as new
    const added = dialog.getByTestId('store-capabilities-new');
    await expect(added).toHaveTextContent('on-render-page');
    await expect(added).not.toHaveTextContent('render-blocks');
    await expect(added).toHaveTextContent('img-src: charts.example.com');
    await userEvent.click(dialog.getByRole('button', { name: 'Install' }));
    await waitFor(() => expect(installs).toHaveLength(1));
    await expect((installs[0].body as { version: string }).version).toBe('0.2.0');
  },
};

export const FailedInstallShowsTheReason: Story = {
  // Its own spy: the meta's spy also counts calls from the other stories
  args: { onInstalled: fn() },
  beforeEach: () =>
    mockApi((req) =>
      req.path === '/store/packages'
        ? listing
        : {
            status: 422,
            body: { error: 'Package does not match the catalog', code: 'package_mismatch' },
          }
    ),
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Install Ad & Banner Delivery' })
    );
    const dialog = within(await within(document.body).findByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Install' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        'The downloaded package does not match the catalog, so it was not installed.',
        'error'
      )
    );
    await expect(args.onInstalled).not.toHaveBeenCalled();
  },
};

export const CatalogRejected: Story = {
  beforeEach: storeApi({
    catalogs: [
      {
        name: 'official',
        url: 'https://takumi-mr.github.io/AegisCMS-Store/catalog.json',
        ok: false,
        code: 'catalog_bad_signature',
        error: 'Catalog signature does not verify',
      },
    ],
    packages: [],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent(
      "The catalog “official” could not be used: The catalog's signature is not valid."
    );
  },
};

export const StoreDisabled: Story = {
  beforeEach: storeApi({
    status: 404,
    body: { error: 'The package store is disabled', code: 'store_disabled' },
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText(/The package store is turned off on this server/)
    ).toBeVisible();
  },
};

export const NotAdmin: Story = {
  args: { canInstall: false },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('Only administrators can install from the store.')).toBeVisible();
  },
};

/** A package with the same ID from another source (a fork, an upload) is never offered the store's version */
export const AnotherSourceIsNotUpdated: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const card = await canvas.findByRole('article', { name: 'Forked Plugin' });
    await expect(
      within(card).getByText('Installed (v1.0.0, from another source: not updated from here)')
    ).toBeVisible();
    await expect(within(card).queryByRole('button')).toBeNull();
  },
};

/** A server that missed the announcement of a new catalog key is told what to do */
export const CatalogKeyRotated: Story = {
  beforeEach: storeApi({
    catalogs: [
      {
        name: 'official',
        url: 'https://takumi-mr.github.io/AegisCMS-Store/catalog.json',
        ok: false,
        code: 'catalog_key_rotated',
        error: 'The catalog is signed with a new key that this server does not know yet',
      },
    ],
    packages: [],
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole('alert')).toHaveTextContent(
      /moved to a new signing key that this server does not know yet\. Update AegisCMS/
    );
  },
};
