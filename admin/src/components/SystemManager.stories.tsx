import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { SystemSettingsResponse, SystemOverrides } from '../types';
import { SystemManager } from './SystemManager';

const defaultResponse: SystemSettingsResponse = {
  settings: {
    maintenance_mode: false,
    maintenance_message: 'Site is currently under maintenance. Please check back soon.',
    max_upload_mb: 10,
    cache_enabled: true,
    cache_ttl_secs: 3600,
    registration_enabled: false,
    max_login_attempts: 20,
    comments_enabled: true,
    forum_enabled: true,
  },
  overrides: {
    maintenance_mode: null,
    maintenance_message: null,
    max_upload_mb: null,
    cache_enabled: null,
    cache_ttl_secs: null,
    registration_enabled: null,
    max_login_attempts: null,
    comments_enabled: null,
    forum_enabled: null,
  },
  defaults: {
    maintenance_mode: false,
    maintenance_message: 'Site is currently under maintenance. Please check back soon.',
    max_upload_mb: 10,
    cache_enabled: true,
    cache_ttl_secs: 3600,
    registration_enabled: false,
    max_login_attempts: 20,
    comments_enabled: true,
    forum_enabled: true,
  },
};

const meta = {
  title: 'Settings/SystemManager',
  component: SystemManager,
  parameters: { layout: 'fullscreen' },
  args: {
    onShowToast: fn(),
  },
} satisfies Meta<typeof SystemManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    let current = JSON.parse(JSON.stringify(defaultResponse)) as SystemSettingsResponse;
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/system/settings') {
        return current;
      }
      if (req.method === 'PUT' && req.path === '/system/settings') {
        const body = req.body as Partial<SystemOverrides>;
        current = {
          ...current,
          overrides: { ...current.overrides, ...body },
          settings: {
            ...current.settings,
            maintenance_mode: body.maintenance_mode ?? current.defaults.maintenance_mode,
            maintenance_message: body.maintenance_message ?? current.defaults.maintenance_message,
            max_upload_mb: body.max_upload_mb ?? current.defaults.max_upload_mb,
            cache_enabled: body.cache_enabled ?? current.defaults.cache_enabled,
            cache_ttl_secs: body.cache_ttl_secs ?? current.defaults.cache_ttl_secs,
            registration_enabled:
              body.registration_enabled ?? current.defaults.registration_enabled,
            max_login_attempts: body.max_login_attempts ?? current.defaults.max_login_attempts,
            comments_enabled: body.comments_enabled ?? current.defaults.comments_enabled,
            forum_enabled: body.forum_enabled ?? current.defaults.forum_enabled,
          },
        };
        return current;
      }
      return undefined;
    });
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const maintenanceHeading = await canvas.findByText('Maintenance Mode');
    await expect(maintenanceHeading).toBeVisible();

    const uploadInput = canvas.getByLabelText(/Max upload size/i);
    await expect(uploadInput).toHaveValue(10);

    const saveButton = canvas.getByRole('button', { name: /Save Changes/i });
    await expect(saveButton).toBeVisible();

    await userEvent.click(saveButton);
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        expect.stringMatching(/System settings saved|システム設定を保存しました/)
      )
    );
  },
};
