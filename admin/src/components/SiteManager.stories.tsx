import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { AdjacentArticlesSettings, SiteSettings } from '../types';
import { SiteManager } from './SiteManager';

const defaultSettings: SiteSettings = {
  title: 'AegisCMS Demo',
  favicon: '/favicon.svg',
  defaults: {
    title: 'AegisCMS Default',
    favicon: '/favicon.svg',
  },
};

const adjacentDefaults: AdjacentArticlesSettings = {
  same_category: null,
  fallback_to_all: null,
  order_by: null,
  defaults: { same_category: true, fallback_to_all: true, order_by: 'published_at' },
};

/** Bodies sent to `PUT /site/adjacent-articles` */
let adjacentPuts: unknown[] = [];

/** Answers the previous/next article settings; `null` values fall back to `defaults` */
const adjacentApi = (
  req: { method: string; path: string; body: unknown },
  state: { current: AdjacentArticlesSettings }
) => {
  if (req.path !== '/site/adjacent-articles') return undefined;
  if (req.method === 'PUT') {
    adjacentPuts.push(req.body);
    state.current = { ...state.current, ...(req.body as Partial<AdjacentArticlesSettings>) };
  }
  return state.current;
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
    const adjacent = { current: { ...adjacentDefaults } };
    return mockApi((req) => {
      const answer = adjacentApi(req, adjacent);
      if (answer) return answer;
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

export const PreviousAndNextArticles: Story = {
  beforeEach: () => {
    adjacentPuts = [];
    const adjacent = { current: { ...adjacentDefaults } };
    return mockApi((req) => (req.path === '/site' ? defaultSettings : adjacentApi(req, adjacent)));
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const form = within(await canvas.findByRole('form', { name: /Previous and next articles/ }));
    const sameCategory = form.getByLabelText('Pick from the same category');
    // The configuration file's value is shown in the first choice
    await expect(
      await within(sameCategory).findByRole('option', { name: 'Configuration file (Yes)' })
    ).toBeInTheDocument();
    await expect(form.getByRole('button', { name: 'Save previous/next settings' })).toBeDisabled();

    await userEvent.selectOptions(sameCategory, 'false');
    await userEvent.selectOptions(form.getByLabelText('Order by'), 'created_at');
    await userEvent.click(form.getByRole('button', { name: 'Save previous/next settings' }));

    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Previous/next article settings saved')
    );
    // Untouched settings are sent as null (the configuration file's value)
    await expect(adjacentPuts).toEqual([
      { same_category: false, fallback_to_all: null, order_by: 'created_at' },
    ]);
    await expect(form.getByRole('button', { name: 'Save previous/next settings' })).toBeDisabled();
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
