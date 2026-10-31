import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

import { mockApi } from '../../.storybook/mockApi';
import type { ContentSummary, NewsletterIssue, NewsletterSubscriber } from '../types';
import { NewsletterManager } from './NewsletterManager';

const subscriber = (
  overrides: Partial<NewsletterSubscriber> & { id: string; email: string }
): NewsletterSubscriber => ({
  status: 'active',
  ip: '203.0.113.7',
  created_at: '2026-10-01T00:00:00Z',
  confirmed_at: '2026-10-01T00:05:00Z',
  unsubscribed_at: null,
  ...overrides,
});

const SUBSCRIBERS: NewsletterSubscriber[] = [
  subscriber({ id: 's1', email: 'hanako@example.com' }),
  subscriber({ id: 's2', email: 'taro@example.com' }),
  subscriber({ id: 's3', email: 'waiting@example.com', status: 'pending', confirmed_at: null }),
  subscriber({ id: 's4', email: 'gone@example.com', status: 'unsubscribed' }),
];

const ARTICLES = [
  { id: 'a1', title: 'The autumn blend is here', excerpt: 'Beans have arrived.' },
  { id: 'a2', title: 'Brewing at home', excerpt: 'Three tips.' },
] as ContentSummary[];

const ISSUES: NewsletterIssue[] = [
  {
    id: 'i1',
    content_type: 'article',
    content_id: 'a2',
    title: 'Brewing at home',
    url: 'https://example.com/brewing',
    recipients: 2,
    created_at: '2026-10-10T00:00:00Z',
    created_by: 'admin',
  },
];

const counts = (list: NewsletterSubscriber[]) => ({
  active: list.filter((s) => s.status === 'active').length,
  pending: list.filter((s) => s.status === 'pending').length,
  unsubscribed: list.filter((s) => s.status === 'unsubscribed').length,
});

/** Serves the newsletter endpoints from in-memory lists, like the server does */
const newsletterApi = (
  initial: NewsletterSubscriber[],
  options: { sentIds?: string[]; sends?: unknown[] } = {}
) => {
  let list = structuredClone(initial);
  const sent = new Set(options.sentIds ?? ISSUES.map((i) => i.content_id));
  return mockApi(({ method, path, body }) => {
    if (path === '/newsletter/subscribers' && method === 'GET') {
      return { subscribers: list, total: list.length, counts: counts(list) };
    }
    if (path === '/newsletter/subscribers/delete' && method === 'POST') {
      const ids = (body as { ids: string[] }).ids;
      list = list.filter((s) => !ids.includes(s.id));
      return { count: ids.length };
    }
    if (path === '/newsletter/issues' && method === 'GET') {
      return { issues: ISSUES, total: ISSUES.length };
    }
    if (path === '/newsletter/issues' && method === 'POST') {
      const input = body as { slug_or_id: string; test_to?: string; force?: boolean };
      options.sends?.push(input);
      if (input.test_to) return { status: 202, body: { status: 'test_sent' } };
      if (sent.has(input.slug_or_id) && !input.force) {
        return {
          status: 409,
          body: { error: 'This content was already sent on 2026-10-10', code: 'already_sent' },
        };
      }
      return { status: 201, body: { recipients: counts(list).active } };
    }
    if (path === '/contents/article' && method === 'GET') {
      return { contents: ARTICLES, total: ARTICLES.length };
    }
    return undefined;
  });
};

const meta = {
  title: 'Settings/NewsletterManager',
  component: NewsletterManager,
  args: { onShowToast: fn() },
  beforeEach: () => newsletterApi(SUBSCRIBERS),
} satisfies Meta<typeof NewsletterManager>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Counts per status, the subscribers with their status, and what was sent before */
export const Default: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('hanako@example.com')).toBeInTheDocument();
    await expect(canvas.getByText('waiting@example.com')).toBeInTheDocument();
    // The history
    await expect(await canvas.findByText('2 recipients')).toBeInTheDocument();
    await expect(canvas.getByRole('button', { name: 'Send to all subscribers' })).toHaveTextContent(
      'Send to 2 subscribers'
    );
  },
};

export const Empty: Story = {
  beforeEach: () => newsletterApi([]),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByText('No subscribers yet.')).toBeInTheDocument();
    // Nobody to send to
    await expect(canvas.getByRole('button', { name: 'Send to all subscribers' })).toBeDisabled();
  },
};

/** The server answers 404 `newsletter_disabled` */
export const Disabled: Story = {
  beforeEach: () =>
    mockApi(({ path }) =>
      path === '/newsletter/subscribers'
        ? {
            status: 404,
            body: { error: 'The newsletter is not enabled', code: 'newsletter_disabled' },
          }
        : undefined
    ),
  play: async ({ canvasElement }) => {
    await expect(
      await within(canvasElement).findByText(/The newsletter is not enabled on the server/)
    ).toBeInTheDocument();
  },
};

/** Sends a test mail to one address first */
export const SendTest: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('hanako@example.com');
    // Nothing is sent before an article is chosen
    await userEvent.click(canvas.getByRole('button', { name: 'Send a test' }));
    await expect(args.onShowToast).toHaveBeenCalledWith('Choose an article first', 'error');

    await userEvent.selectOptions(
      await canvas.findByLabelText('Article to send'),
      'The autumn blend is here'
    );
    await userEvent.click(canvas.getByRole('button', { name: 'Send a test' }));
    await expect(args.onShowToast).toHaveBeenCalledWith(
      'Enter an email address for the test',
      'error'
    );
    await userEvent.type(canvas.getByLabelText('Email address for the test'), 'me@example.com');
    await userEvent.click(canvas.getByRole('button', { name: 'Send a test' }));
    await waitFor(() =>
      expect(args.onShowToast).toHaveBeenCalledWith('Sent a test mail to me@example.com')
    );
  },
};

/** Sends an article to everyone after a confirmation */
export const SendToSubscribers: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('hanako@example.com');
    await userEvent.selectOptions(
      await canvas.findByLabelText('Article to send'),
      'The autumn blend is here'
    );
    window.confirm = () => true;
    await userEvent.click(canvas.getByRole('button', { name: 'Send to all subscribers' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Sent to 2 subscribers'));
  },
};

/** Content that was sent before asks again, and is sent with `force` only after the answer */
export const SendAgain: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('hanako@example.com');
    await userEvent.selectOptions(
      await canvas.findByLabelText('Article to send'),
      'Brewing at home'
    );
    const questions: string[] = [];
    window.confirm = (question?: string) => {
      questions.push(question ?? '');
      return true;
    };
    await userEvent.click(canvas.getByRole('button', { name: 'Send to all subscribers' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Sent to 2 subscribers'));
    await expect(questions).toHaveLength(2);
    await expect(questions[1]).toContain('was already sent on');
  },
};

/** Declining the confirmation sends nothing */
export const DeclineToSend: Story = {
  beforeEach: () => {
    const sends: unknown[] = [];
    (globalThis as { __sends?: unknown[] }).__sends = sends;
    return newsletterApi(SUBSCRIBERS, { sends });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('hanako@example.com');
    await userEvent.selectOptions(
      await canvas.findByLabelText('Article to send'),
      'The autumn blend is here'
    );
    window.confirm = () => false;
    await userEvent.click(canvas.getByRole('button', { name: 'Send to all subscribers' }));
    await expect((globalThis as { __sends?: unknown[] }).__sends).toHaveLength(0);
  },
};

/** Deletes a subscriber after a confirmation */
export const DeleteSubscriber: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('gone@example.com');
    window.confirm = () => true;
    await userEvent.click(canvas.getByRole('button', { name: 'Delete gone@example.com' }));
    await waitFor(() => expect(args.onShowToast).toHaveBeenCalledWith('Deleted gone@example.com'));
    await waitFor(() => expect(canvas.queryByText('gone@example.com')).not.toBeInTheDocument());
  },
};

/** Narrows the list by status */
export const FilterByStatus: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByText('hanako@example.com');
    await userEvent.selectOptions(canvas.getByLabelText('Status'), 'pending');
    // The mock returns the same list, so only the control is checked here
    await expect(canvas.getByLabelText('Status')).toHaveValue('pending');
  },
};
