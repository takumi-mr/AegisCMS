import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { Redirect } from '../types';
import { RedirectManager } from './RedirectManager';

const rule = (
  overrides: Partial<Redirect> & { id: string; from: string; to: string }
): Redirect => ({
  status: 301,
  auto: false,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
  ...overrides,
});

const RULES: Redirect[] = [
  rule({
    id: 'r3',
    from: '/articles/hello',
    to: '/articles/hello-2',
    auto: true,
    created_at: '2026-10-03T00:00:00Z',
  }),
  rule({
    id: 'r2',
    from: '/campaign',
    to: 'https://example.com/spring',
    status: 302,
    created_at: '2026-10-02T00:00:00Z',
  }),
  rule({ id: 'r1', from: '/old-about', to: '/about' }),
];

/** Serves the redirect endpoints from an in-memory list, like the server does */
const redirectApi = (initial: Redirect[]) => {
  let rules = structuredClone(initial);
  return mockApi(({ method, path, body }) => {
    if (path === '/redirects' && method === 'GET') {
      return { redirects: rules, total: rules.length };
    }
    if (path === '/redirects' && method === 'POST') {
      const input = body as { from: string; to: string; status: number };
      if (rules.some((r) => r.from === input.from)) {
        return {
          status: 409,
          body: {
            error: 'A redirect from this path already exists',
            code: 'duplicate_redirect',
          },
        };
      }
      const created = rule({ id: `r${rules.length + 10}`, ...input });
      rules = [created, ...rules];
      return { status: 201, body: created };
    }
    const one = path.match(/^\/redirects\/([^/]+)$/);
    if (one && method === 'PUT') {
      const input = body as { from: string; to: string; status: number };
      rules = rules.map((r) => (r.id === one[1] ? { ...r, ...input, auto: false } : r));
      return rules.find((r) => r.id === one[1]);
    }
    if (one && method === 'DELETE') {
      rules = rules.filter((r) => r.id !== one[1]);
      return { status: 204, body: null };
    }
    return undefined;
  });
};

const meta = {
  title: 'Settings/RedirectManager',
  component: RedirectManager,
  args: { onShowToast: fn() },
  beforeEach: () => redirectApi(RULES),
} satisfies Meta<typeof RedirectManager>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Manual and automatic redirects, permanent and temporary ones */
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('/articles/hello')).toBeInTheDocument();
    await expect(canvas.getByText('/campaign')).toBeInTheDocument();
    // Only the redirect kept by a slug change is marked
    await expect(canvas.getAllByText('Auto')).toHaveLength(1);
  },
};

export const Empty: Story = {
  beforeEach: () => redirectApi([]),
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByText('No redirects yet.')).toBeInTheDocument();
  },
};

/** Adds a redirect, then tries the same path again */
export const AddRedirect: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('/old-about');
    await userEvent.type(canvas.getByLabelText('Old path'), '/gone');
    await userEvent.type(canvas.getByLabelText('New path or URL'), '/articles/new');
    await userEvent.selectOptions(canvas.getByLabelText('Redirect type'), '302');
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Added a redirect from /gone')
    );
    // The list now shows the new one (the search is set to its path)
    await expect(await canvas.findByText('/articles/new')).toBeInTheDocument();

    // The same path again is refused with the message for its code
    await userEvent.type(canvas.getByLabelText('Old path'), '/gone');
    await userEvent.type(canvas.getByLabelText('New path or URL'), '/other');
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await expect(
      await canvas.findByText('A redirect from this path already exists')
    ).toBeInTheDocument();
  },
};

/** Changes a redirect, then deletes it */
export const EditAndDelete: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('/old-about');
    await userEvent.click(
      canvas.getByRole('button', { name: 'Edit the redirect from /old-about' })
    );
    await expect(canvas.getByLabelText('Old path')).toHaveValue('/old-about');
    const to = canvas.getByLabelText('New path or URL');
    await userEvent.clear(to);
    await userEvent.type(to, '/about-us');
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Changed the redirect from /old-about')
    );
    await expect(await canvas.findByText('/about-us')).toBeInTheDocument();

    window.confirm = () => true;
    await userEvent.click(
      canvas.getByRole('button', { name: 'Delete the redirect from /old-about' })
    );
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Deleted the redirect from /old-about')
    );
    await waitFor(() => expect(canvas.queryByText('/about-us')).not.toBeInTheDocument());
  },
};
