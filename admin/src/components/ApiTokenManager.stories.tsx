import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { ApiToken } from '../types';
import { ApiTokenManager } from './ApiTokenManager';

const existing: ApiToken = {
  id: 'tok-1',
  name: 'claude-code',
  prefix: 'aegis_pat_3f9a',
  role: 'editor',
  created_at: '2026-11-01T00:00:00Z',
  expires_at: '2027-01-30T00:00:00Z',
  last_used_at: '2026-11-11T08:00:00Z',
};

/** Tokens kept in memory: GET lists, POST issues one (with the plaintext), DELETE revokes */
const tokenApi = (initial: ApiToken[]) => {
  let tokens = [...initial];
  return mockApi((req) => {
    if (req.path === '/users/me/tokens' && req.method === 'GET') return { tokens };
    if (req.path === '/users/me/tokens' && req.method === 'POST') {
      const body = req.body as { name: string; role: ApiToken['role'] };
      const record: ApiToken = {
        id: `tok-${tokens.length + 2}`,
        name: body.name,
        prefix: 'aegis_pat_ab12',
        role: body.role,
        created_at: '2026-11-12T00:00:00Z',
        expires_at: '2027-02-10T00:00:00Z',
        last_used_at: null,
      };
      tokens = [record, ...tokens];
      return { status: 201, body: { ...record, token: 'aegis_pat_ab12-the-whole-secret-value' } };
    }
    const one = req.path.match(/^\/users\/me\/tokens\/([^/]+)$/);
    if (one && req.method === 'DELETE') {
      tokens = tokens.filter((t) => t.id !== one[1]);
      return { status: 204, body: null };
    }
    return undefined;
  });
};

const meta = {
  title: 'Users/ApiTokenManager',
  component: ApiTokenManager,
  args: { maxRole: 'editor', onShowToast: fn() },
} satisfies Meta<typeof ApiTokenManager>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Empty: Story = {
  beforeEach: () => tokenApi([]),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('No API tokens yet.')).toBeVisible();
  },
};

export const IssueShowsTheTokenOnce: Story = {
  beforeEach: () => tokenApi([existing]),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('claude-code');
    await userEvent.type(canvas.getByPlaceholderText('e.g. claude-code'), 'nightly-ci');
    await userEvent.click(canvas.getByRole('button', { name: 'Create token' }));

    const shown = await canvas.findByTestId('created-token');
    await expect(shown).toHaveTextContent('aegis_pat_ab12-the-whole-secret-value');
    await expect(await canvas.findByText('nightly-ci')).toBeVisible();

    // Closing the notice drops the token from the page; the list only has the prefix
    await userEvent.click(canvas.getByRole('button', { name: /I have saved it/ }));
    await waitFor(() => expect(canvas.queryByTestId('created-token')).toBeNull());
    await expect(canvas.queryByText(/the-whole-secret-value/)).toBeNull();
  },
};

export const Revoke: Story = {
  beforeEach: () => {
    const cleanup = tokenApi([existing]);
    const confirm = window.confirm;
    window.confirm = () => true;
    return () => {
      window.confirm = confirm;
      cleanup();
    };
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(
      await canvas.findByRole('button', { name: 'Revoke the API token claude-code' })
    );
    await expect(await canvas.findByText('No API tokens yet.')).toBeVisible();
  },
};

export const ContributorCannotPickEditor: Story = {
  args: { maxRole: 'contributor' },
  beforeEach: () => tokenApi([]),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('No API tokens yet.');
    const select = canvas.getByRole('combobox', { name: 'Highest role' });
    const options = within(select)
      .getAllByRole('option')
      .map((o) => o.textContent);
    await expect(options).toHaveLength(2);
    await expect(options.join(' ')).not.toMatch(/editor/i);
  },
};
