import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { AdminComment, CommentPage, CommentSettings, CommentStatus } from '../types';
import { CommentManager } from './CommentManager';

const comment = (
  id: string,
  status: CommentStatus,
  author: string,
  body: string
): AdminComment => ({
  id,
  content_type: 'article',
  content_id: 'a1',
  content: {
    title: 'Hello world',
    slug: 'hello-world',
    url: '/articles/hello-world',
    comments_open: true,
  },
  parent_id: null,
  parent_author: null,
  author_name: author,
  author_email: `${author.toLowerCase()}@example.com`,
  author_user_id: null,
  body,
  status,
  ip: '203.0.113.7',
  user_agent: 'Mozilla/5.0',
  notify_replies: false,
  created_at: '2026-11-11T09:00:00Z',
  updated_at: '2026-11-11T09:00:00Z',
});

const policy = { require_approval: true, auto_approve_known: false };

const handler: MockHandler = (req) => {
  if (req.path === '/comments/settings') {
    return {
      policy,
      overrides: { require_approval: null, auto_approve_known: null },
      defaults: policy,
      content_types: ['article'],
    } satisfies CommentSettings;
  }
  if (req.path === '/comments') {
    const comments = [
      comment('c1', 'pending', 'Hanako', 'Thanks for the write-up!'),
      comment('c2', 'pending', 'Taro', 'Could you cover the CLI too?'),
    ];
    return {
      comments,
      total: comments.length,
      counts: { pending: 2, approved: 5, spam: 1, trash: 0 },
    } satisfies CommentPage;
  }
  return undefined;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Moderation/CommentManager',
  component: CommentManager,
  args: { isAdmin: true, onShowToast: fn() },
} satisfies Meta<typeof CommentManager>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Admin: Story = {};
export const Editor: Story = { args: { isAdmin: false } };
