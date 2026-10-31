import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { Notice, NoticePayload } from '../types';
import { NoticeManager } from './NoticeManager';

const notice = (overrides: Partial<Notice> & { id: string; message: string }): Notice => ({
  link_url: '',
  link_label: '',
  style: 'info',
  start_at: null,
  end_at: null,
  dismissible: true,
  enabled: true,
  status: 'active',
  created_at: '2026-11-01T00:00:00Z',
  updated_at: '2026-11-01T00:00:00Z',
  ...overrides,
});

const NOTICES: Notice[] = [
  notice({
    id: 'n1',
    message: 'Maintenance tonight from 0:00 to 2:00.',
    style: 'warning',
    link_url: '/status',
    end_at: '2999-01-01T00:00:00Z',
  }),
  notice({
    id: 'n2',
    message: 'Autumn sale starts next month.',
    start_at: '2999-01-01T00:00:00Z',
    status: 'scheduled',
  }),
  notice({
    id: 'n3',
    message: 'Last year’s sale.',
    end_at: '2020-01-01T00:00:00Z',
    status: 'expired',
  }),
  notice({ id: 'n4', message: 'A draft.', enabled: false, status: 'disabled', dismissible: false }),
];

/** Serves the notice endpoints from an in-memory list, like the server does */
const noticeApi = (initial: Notice[], sent: NoticePayload[] = []) => {
  let list = structuredClone(initial);
  return mockApi(({ method, path, body }) => {
    if (path === '/notices' && method === 'GET') return { notices: list, total: list.length };
    if (path === '/notices' && method === 'POST') {
      const payload = body as NoticePayload;
      sent.push(payload);
      if (payload.message.includes('<bad>')) {
        return {
          status: 400,
          body: {
            error: 'must be 500 characters or fewer',
            code: 'invalid_notice',
            field: 'message',
          },
        };
      }
      const created = notice({
        id: `n${list.length + 10}`,
        ...payload,
        status: payload.enabled ? 'active' : 'disabled',
      });
      list = [...list, created];
      return { status: 201, body: created };
    }
    const one = path.match(/^\/notices\/([^/]+)$/);
    if (one && method === 'PUT') {
      sent.push(body as NoticePayload);
      list = list.map((n) => (n.id === one[1] ? { ...n, ...(body as NoticePayload) } : n));
      return list.find((n) => n.id === one[1]);
    }
    if (one && method === 'DELETE') {
      list = list.filter((n) => n.id !== one[1]);
      return { status: 204, body: null };
    }
    return undefined;
  });
};

const meta = {
  title: 'Settings/NoticeManager',
  component: NoticeManager,
  args: { onShowToast: fn() },
  beforeEach: () => noticeApi(NOTICES),
} satisfies Meta<typeof NoticeManager>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Every state a notice can be in, with its period and link */
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      await canvas.findByText('Maintenance tonight from 0:00 to 2:00.')
    ).toBeInTheDocument();
    await expect(canvas.getByText('Showing')).toBeInTheDocument();
    await expect(canvas.getByText('Scheduled')).toBeInTheDocument();
    await expect(canvas.getByText('Ended')).toBeInTheDocument();
    await expect(canvas.getByText('Off')).toBeInTheDocument();
    // The link and the "cannot be closed" mark are part of the line under the message
    await expect(canvas.getByText(/\/status/)).toBeInTheDocument();
    await expect(canvas.getByText(/cannot be closed/)).toBeInTheDocument();
  },
};

export const Empty: Story = {
  beforeEach: () => noticeApi([]),
  play: async ({ canvasElement }) => {
    await expect(await within(canvasElement).findByText('No notices yet.')).toBeInTheDocument();
  },
};

/** More notices are active than the site shows at once */
export const TooManyActive: Story = {
  beforeEach: () =>
    noticeApi([1, 2, 3, 4].map((n) => notice({ id: `a${n}`, message: `Active ${n}` }))),
  play: async ({ canvasElement }) => {
    await expect(
      await within(canvasElement).findByText(/More notices are active than the 3/)
    ).toBeInTheDocument();
  },
};

/** Adds a notice with a period; the form sends UTC timestamps */
export const AddNotice: Story = {
  beforeEach: () => {
    const sent: NoticePayload[] = [];
    (globalThis as { __sent?: NoticePayload[] }).__sent = sent;
    return noticeApi(NOTICES, sent);
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('Maintenance tonight from 0:00 to 2:00.');

    // The message is required, and the end has to come after the start
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await expect(await canvas.findByText('Enter the message')).toBeInTheDocument();
    await userEvent.type(canvas.getByLabelText('Message of the notice'), 'Free shipping this week');
    await userEvent.type(canvas.getByLabelText('Show from'), '2026-12-02T09:00');
    await userEvent.type(canvas.getByLabelText('Show until'), '2026-12-01T09:00');
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await expect(await canvas.findByText('The end must be after the start')).toBeInTheDocument();

    await userEvent.clear(canvas.getByLabelText('Show until'));
    await userEvent.type(canvas.getByLabelText('Show until'), '2026-12-08T09:00');
    await userEvent.type(canvas.getByLabelText('Link address (a path or an URL)'), '/shipping');
    await userEvent.selectOptions(canvas.getByLabelText('Kind of notice'), 'success');
    await userEvent.click(canvas.getByRole('checkbox', { name: 'Visitors can close it' }));
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Added the notice'));
    const sent = (globalThis as { __sent?: NoticePayload[] }).__sent!;
    await expect(sent).toHaveLength(1);
    await expect(sent[0]).toMatchObject({
      message: 'Free shipping this week',
      link_url: '/shipping',
      style: 'success',
      dismissible: false,
      enabled: true,
    });
    // local time went to the server as UTC timestamps
    await expect(sent[0].start_at).toBe(new Date('2026-12-02T09:00').toISOString());
    await expect(sent[0].end_at).toBe(new Date('2026-12-08T09:00').toISOString());
    await expect(await canvas.findByText('Free shipping this week')).toBeInTheDocument();
  },
};

/** The server's reason is shown with the field it refused, named in the UI language */
export const ShowsTheRefusedField: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('Maintenance tonight from 0:00 to 2:00.');
    await userEvent.type(canvas.getByLabelText('Message of the notice'), '<bad> message');
    await userEvent.click(canvas.getByRole('button', { name: 'Add' }));
    await expect(
      await canvas.findByText('Message: must be 500 characters or fewer')
    ).toBeInTheDocument();
  },
};

/** Editing fills the form from the notice (dates in local time), and saving changes it */
export const EditAndDelete: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('Maintenance tonight from 0:00 to 2:00.');
    await userEvent.click(
      canvas.getByRole('button', {
        name: 'Edit the notice: Maintenance tonight from 0:00 to 2:00.',
      })
    );
    await expect(canvas.getByLabelText('Message of the notice')).toHaveValue(
      'Maintenance tonight from 0:00 to 2:00.'
    );
    await expect(canvas.getByLabelText('Kind of notice')).toHaveValue('warning');
    await expect(canvas.getByLabelText('Link address (a path or an URL)')).toHaveValue('/status');
    await expect(canvas.getByLabelText('Show until')).toHaveValue(
      (await import('../utils/format')).toDateTimeLocal('2999-01-01T00:00:00Z')
    );
    const message = canvas.getByLabelText('Message of the notice');
    await userEvent.clear(message);
    await userEvent.type(message, 'Maintenance is over');
    await userEvent.click(canvas.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Changed the notice'));
    await expect(await canvas.findByText('Maintenance is over')).toBeInTheDocument();

    window.confirm = () => true;
    await userEvent.click(canvas.getByRole('button', { name: 'Delete the notice: A draft.' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Deleted the notice'));
    await waitFor(() => expect(canvas.queryByText('A draft.')).not.toBeInTheDocument());
  },
};
