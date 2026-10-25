import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { SiteSettings } from '../types';
import { SiteManager } from './SiteManager';

const defaultSettings: SiteSettings = {
  title: 'AegisCMS Demo',
  favicon: '/favicon.svg',
  defaults: {
    title: 'AegisCMS Default',
    favicon: '/favicon.svg',
  },
};

const meta = {
  title: 'Settings/SiteManager',
  component: SiteManager,
  parameters: { layout: 'fullscreen' },
  args: {
    onShowToast: fn(),
  },
} satisfies Meta<typeof SiteManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    let current = { ...defaultSettings };
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/site') {
        return current;
      }
      if (req.method === 'PUT' && req.path === '/site') {
        current = { ...current, ...(req.body as Partial<SiteSettings>) };
        return current;
      }
      return undefined;
    });
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const input = await canvas.findByDisplayValue('AegisCMS Demo');
    await expect(input).toBeVisible();

    await userEvent.clear(input);
    await userEvent.type(input, 'New Aegis Brand Title');

    const saveButton = canvas.getByRole('button', { name: 'Save' });
    await expect(saveButton).toBeEnabled();
    await userEvent.click(saveButton);

    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Site settings saved'));
  },
};

export const Empty: Story = {
  beforeEach: () => {
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/site') {
        return {
          title: null,
          favicon: null,
          defaults: { title: 'AegisCMS Default', favicon: null },
        };
      }
      return undefined;
    });
  },
};
