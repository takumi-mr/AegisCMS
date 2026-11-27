import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { HeaderSettings } from '../types';
import { HeaderManager } from './HeaderManager';

const defaultHeaderSettings: HeaderSettings = {
  content_security_policy: null,
  x_frame_options: null,
  x_content_type_options: null,
  referrer_policy: null,
  permissions_policy: null,
  cross_origin_opener_policy: null,
  custom_headers: [{ name: 'X-Powered-By', value: 'AegisCMS' }],
  embed_frame_domains: [],
  config_embed_frame_domains: [],
  embed_frame_domains_in_use: true,
  defaults: {
    content_security_policy: "default-src 'self'; script-src 'self' 'unsafe-inline';",
    x_frame_options: 'SAMEORIGIN',
    x_content_type_options: 'nosniff',
    referrer_policy: 'strict-origin-when-cross-origin',
    permissions_policy: 'camera=(), microphone=()',
    cross_origin_opener_policy: 'same-origin',
  },
};

const meta = {
  title: 'Settings/HeaderManager',
  component: HeaderManager,
  parameters: { layout: 'fullscreen' },
  args: {
    onShowToast: fn(),
  },
} satisfies Meta<typeof HeaderManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    let current = { ...defaultHeaderSettings };
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/headers') {
        return current;
      }
      if (req.method === 'PUT' && req.path === '/headers') {
        current = { ...current, ...(req.body as Partial<HeaderSettings>) };
        return current;
      }
      return undefined;
    });
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const customInput = await canvas.findByDisplayValue('X-Powered-By');
    await expect(customInput).toBeVisible();

    const xFrameInput = canvas.getByLabelText(/X-Frame-Options/i);
    await userEvent.type(xFrameInput, 'DENY');

    const saveButton = canvas.getByRole('button', { name: /Save/i });
    await expect(saveButton).toBeEnabled();
    await userEvent.click(saveButton);

    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Headers settings saved'));
  },
};

export const Empty: Story = {
  beforeEach: () => {
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/headers') {
        return {
          content_security_policy: null,
          x_frame_options: null,
          x_content_type_options: null,
          referrer_policy: null,
          permissions_policy: null,
          cross_origin_opener_policy: null,
          custom_headers: [],
          embed_frame_domains: [],
          config_embed_frame_domains: [],
          embed_frame_domains_in_use: false,
          defaults: defaultHeaderSettings.defaults,
        };
      }
      return undefined;
    });
  },
};

/** Allowed embed hosts: one per line, sent as a list; warns when no plugin accepts them */
export const EmbedHosts: Story = {
  beforeEach: () => {
    let current: HeaderSettings = {
      ...defaultHeaderSettings,
      embed_frame_domains: ['forms.example.com'],
      config_embed_frame_domains: ['maps.example.org'],
      embed_frame_domains_in_use: false,
    };
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/headers') {
        return current;
      }
      if (req.method === 'PUT' && req.path === '/headers') {
        const body = req.body as { embed_frame_domains?: string[] };
        current = { ...current, embed_frame_domains: body.embed_frame_domains ?? [] };
        return current;
      }
      return undefined;
    });
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const hosts = await canvas.findByLabelText(/Allowed hosts/i);
    await waitFor(() => expect(hosts).toHaveValue('forms.example.com'));
    await expect(canvas.getByText(/maps\.example\.org/)).toBeVisible();
    await expect(canvas.getByRole('status')).toHaveTextContent(/No enabled plugin/);

    await userEvent.type(hosts, '\n\n  calendar.example.net  ');
    await userEvent.click(canvas.getByRole('button', { name: /Save/i }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Headers settings saved'));
    await expect(hosts).toHaveValue('forms.example.com\ncalendar.example.net');
  },
};
