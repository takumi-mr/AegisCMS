import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { PublicPathEntry } from '../types';
import { PublicPathManager } from './PublicPathManager';

const entries: PublicPathEntry[] = [
  {
    content_type: 'article',
    label: 'Articles',
    running: '/articles',
    file: '/articles',
    override: null,
    configured: '/articles',
    restart_required: false,
  },
  {
    content_type: 'memo',
    label: 'Memos',
    running: null,
    file: null,
    override: null,
    configured: null,
    restart_required: false,
  },
  {
    content_type: 'works',
    label: 'Works',
    running: '/works',
    file: '/works',
    override: null,
    configured: '/works',
    restart_required: false,
  },
];

const meta = {
  title: 'Settings/PublicPathManager',
  component: PublicPathManager,
  parameters: { layout: 'fullscreen' },
  args: {
    onShowToast: fn(),
  },
} satisfies Meta<typeof PublicPathManager>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Saving a value shows what runs now, what applies after the restart, and the restart notice */
export const SaveAndRestartNotice: Story = {
  beforeEach: () => {
    let current = entries.map((e) => ({ ...e }));
    return mockApi((req) => {
      if (req.method === 'GET' && req.path === '/public-paths') return current;
      if (req.method === 'PUT' && req.path === '/public-paths/article') {
        const value = (req.body as { public_path: string | null }).public_path;
        current = current.map((e) =>
          e.content_type === 'article'
            ? {
                ...e,
                override: value,
                configured: value ?? e.file,
                restart_required: (value ?? e.file) !== e.running,
              }
            : e
        );
        return current.find((e) => e.content_type === 'article');
      }
      return undefined;
    });
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const input = await canvas.findByLabelText('Public URL of Articles');
    await expect(canvas.queryByRole('status')).toBeNull();

    await userEvent.type(input, '/news');
    // Every type has its own Save button
    const articleCard = input.closest('form') as HTMLElement;
    await userEvent.click(within(articleCard).getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith(
        'Saved the public URL of Articles. Restart the server to apply it.'
      )
    );
    await expect(
      await canvas.findByText('Restart the server to apply the saved URLs.')
    ).toBeVisible();
    await expect(canvas.getByText('After restart: /news')).toBeVisible();
    await expect(canvas.getByText('In use now: /articles')).toBeVisible();

    // Going back to the schema file's value removes the override and the notice
    await userEvent.click(
      canvas.getByRole('button', { name: "Use the schema file's value for Articles" })
    );
    await waitFor(() => expect(canvas.queryByRole('status')).toBeNull());
    await expect(input).toHaveValue('');
  },
};

/** A type that has no public pages yet can be given a URL */
export const TypeWithoutPublicPages: Story = {
  beforeEach: () =>
    mockApi((req) => (req.method === 'GET' && req.path === '/public-paths' ? entries : undefined)),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByLabelText('Public URL of Memos')).toBeVisible();
    await expect(canvas.getByText('In use now: No public pages')).toBeVisible();
    await expect(
      canvas.getByText('The schema file has no public_path, so there are no public pages.')
    ).toBeVisible();
  },
};

/** The server's validation message is shown as it is */
export const RejectedValue: Story = {
  beforeEach: () =>
    mockApi((req) => {
      if (req.method === 'GET' && req.path === '/public-paths') return entries;
      if (req.method === 'PUT' && req.path === '/public-paths/memo') {
        return {
          status: 400,
          body: { error: "schemas 'memo' and 'works' use the same public_path '/works'" },
        };
      }
      return undefined;
    }),
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await userEvent.type(await canvas.findByLabelText('Public URL of Memos'), '/works');
    const memoCard = canvas.getByLabelText('Public URL of Memos').closest('form') as HTMLElement;
    await userEvent.click(within(memoCard).getByRole('button', { name: 'Save' }));

    await expect(await canvas.findByRole('alert')).toHaveTextContent('use the same public_path');
    await expect(args.onShowToast).not.toHaveBeenCalled();
  },
};
