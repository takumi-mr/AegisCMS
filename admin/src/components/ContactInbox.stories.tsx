import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { ContactMessage, ContactStatus } from '../types';
import { ContactInbox } from './ContactInbox';

const forms = [
  { name: 'default', title: 'Contact', path: '/contact/default', auto_reply: false, fields: [] },
  { name: 'sales', title: 'Sales inquiry', path: '/contact/sales', auto_reply: true, fields: [] },
];

const message = (
  id: string,
  status: ContactStatus,
  name: string,
  subject: string,
  body: string,
  form = 'default'
): ContactMessage => ({
  id,
  form,
  form_title: form === 'sales' ? 'Sales inquiry' : 'Contact',
  name,
  email: `${id}@example.com`,
  subject,
  fields: [
    { name: 'name', label: 'Name', value: name },
    { name: 'email', label: 'Email', value: `${id}@example.com` },
    { name: 'subject', label: 'Subject', value: subject },
    { name: 'message', label: 'Message', value: body },
  ],
  status,
  ip: '203.0.113.7',
  user_agent: 'Mozilla/5.0',
  created_at: '2026-10-28T09:15:00Z',
  updated_at: '2026-10-28T09:15:00Z',
});

/** An inbox kept in memory: the list filters by status, PATCH / bulk / DELETE change it */
const inbox = (initial: ContactMessage[]) => {
  let messages = [...initial];
  const counts = () =>
    messages.reduce<Record<string, number>>((acc, m) => {
      acc[m.status] = (acc[m.status] ?? 0) + 1;
      return acc;
    }, {});
  let lastQuery = '';
  const cleanup = mockApi((req) => {
    if (req.path === '/contact/forms') return forms;
    if (req.method === 'GET' && req.path === '/contact/messages') {
      const params = new URLSearchParams(lastQuery);
      const status = params.get('status');
      const q = (params.get('q') ?? '').toLowerCase();
      const shown = messages
        .filter((m) => !status || m.status === status)
        .filter(
          (m) =>
            !q ||
            [m.name, m.email, m.subject, ...m.fields.map((f) => f.value)].some((v) =>
              v.toLowerCase().includes(q)
            )
        );
      return { messages: shown, total: shown.length, counts: counts() };
    }
    const one = req.path.match(/^\/contact\/messages\/([^/]+)$/);
    if (one && req.method === 'PATCH') {
      const { status } = req.body as { status: ContactStatus };
      messages = messages.map((m) => (m.id === one[1] ? { ...m, status } : m));
      return { status: 204, body: null };
    }
    if (one && req.method === 'DELETE') {
      messages = messages.filter((m) => m.id !== one[1]);
      return { status: 204, body: null };
    }
    if (req.method === 'POST' && req.path === '/contact/messages/bulk') {
      const { ids, action } = req.body as { ids: string[]; action: string };
      messages =
        action === 'delete'
          ? messages.filter((m) => !ids.includes(m.id))
          : messages.map((m) =>
              ids.includes(m.id) ? { ...m, status: action as ContactStatus } : m
            );
      return { count: ids.length };
    }
    return undefined;
  });
  // The handler only sees the path: keep the query string (the status filter) on the way in
  const mocked = window.fetch;
  window.fetch = async (input, init) => {
    const url = String(input instanceof Request ? input.url : input);
    if (url.includes('/contact/messages?')) lastQuery = url.split('?')[1];
    return mocked(input, init);
  };
  return () => {
    window.fetch = mocked;
    cleanup();
  };
};

const meta = {
  title: 'Moderation/ContactInbox',
  component: ContactInbox,
  parameters: { layout: 'fullscreen' },
  args: { onShowToast: fn() },
} satisfies Meta<typeof ContactInbox>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () =>
    inbox([
      message(
        'm1',
        'new',
        'Hanako Yamada',
        'Quote for 10 users',
        'Could you send a quote?\nWe pay by invoice.',
        'sales'
      ),
      message('m2', 'new', 'Taro Sato', 'Login problem', 'I cannot sign in since yesterday.'),
      message('m3', 'done', 'Jiro', 'Thanks', 'Thank you for the quick answer.'),
    ]),
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Quote for 10 users')).toBeVisible();
    await expect(canvas.getByText('Login problem')).toBeVisible();
    // The "done" message is not in the default New tab
    await expect(canvas.queryByText('Thanks')).toBeNull();

    // Opening a new message shows every field and marks it read
    await userEvent.click(canvas.getByText('Quote for 10 users'));
    const dialog = within(await within(document.body).findByRole('dialog'));
    await expect(dialog.getByText(/We pay by invoice/)).toBeVisible();
    await expect(dialog.getByRole('link', { name: 'Reply by email' })).toHaveAttribute(
      'href',
      'mailto:m1%40example.com?subject=Re%3A%20Quote%20for%2010%20users'
    );
    await waitFor(() => expect(dialog.getByText('Read')).toBeVisible());

    await userEvent.click(dialog.getByRole('button', { name: 'Mark as done' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Marked 1 message(s) as done')
    );
    await waitFor(() => expect(canvas.queryByText('Quote for 10 users')).toBeNull());

    // Bulk: select the remaining new message and mark it as spam
    await userEvent.click(canvas.getByRole('checkbox', { name: /Taro Sato/ }));
    const bar = within(canvas.getByRole('toolbar'));
    await userEvent.click(bar.getByRole('button', { name: 'Mark as spam' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Marked 1 message(s) as spam')
    );
    await waitFor(() => expect(canvas.getByText('No messages in "New".')).toBeVisible());
  },
};

export const Empty: Story = {
  beforeEach: () => inbox([]),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('No messages in "New".')).toBeVisible();
    await expect(canvas.getByRole('link', { name: 'Sales inquiry' })).toHaveAttribute(
      'href',
      '/contact/sales'
    );
  },
};

export const Disabled: Story = {
  beforeEach: () => mockApi(() => undefined),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText('Contact forms are not enabled ([contact].enabled).')
    ).toBeVisible();
  },
};

export const SearchAndAttachment: Story = {
  beforeEach: () => {
    const withFile = message('m1', 'new', 'Hanako Yamada', 'Contract review', 'See the file.');
    withFile.fields.push({
      name: 'attachment',
      label: 'Attachment',
      value: 'contract-draft.pdf',
      file: { filename: 'contract-draft.pdf', size: 20480, content_type: 'application/pdf' },
    });
    return inbox([withFile, message('m2', 'new', 'Taro Sato', 'Login problem', 'Cannot sign in.')]);
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('Contract review')).toBeVisible();
    await expect(canvas.getByLabelText('Has an attachment')).toBeInTheDocument();

    // The search filters the list by an answer (here: the attached file name)
    await userEvent.type(canvas.getByRole('searchbox'), 'contract-draft{Enter}');
    await waitFor(() => expect(canvas.queryByText('Login problem')).toBeNull());
    await expect(canvas.getByText('Contract review')).toBeVisible();

    await userEvent.click(canvas.getByText('Contract review'));
    const dialog = within(await within(document.body).findByRole('dialog'));
    await expect(dialog.getByRole('button', { name: /contract-draft\.pdf/ })).toBeVisible();

    await userEvent.keyboard('{Escape}');
    await userEvent.type(canvas.getByRole('searchbox'), '{Backspace>20/}nothing{Enter}');
    await waitFor(() => expect(canvas.getByText('No messages match "nothing".')).toBeVisible());
    await userEvent.click(canvas.getByRole('button', { name: 'Clear' }));
    await waitFor(() => expect(canvas.getByText('Login problem')).toBeVisible());
  },
};
