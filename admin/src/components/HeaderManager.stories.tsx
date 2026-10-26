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
          defaults: defaultHeaderSettings.defaults,
        };
      }
      return undefined;
    });
  },
};
