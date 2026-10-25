import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { mockApi } from '../../.storybook/mockApi';
import type { ForumCategory, ForumPost, ForumThread } from '../types';
import { ForumManager } from './ForumManager';

const mockCategories: ForumCategory[] = [
  {
    id: 'cat-1',
    slug: 'general',
    name: 'General Discussion',
    description: 'Casual talk and community discussions',
    position: 1,
    is_private: false,
    created_at: '2026-10-01T00:00:00Z',
    updated_at: '2026-10-01T00:00:00Z',
  },
  {
    id: 'cat-2',
    slug: 'tech',
    name: 'Tech & Architecture',
    description: 'Deep dives into Wasm and Rust',
    position: 2,
    is_private: false,
    created_at: '2026-10-02T00:00:00Z',
    updated_at: '2026-10-02T00:00:00Z',
  },
];

const mockThreads: ForumThread[] = [
  {
    id: 'th-1',
    category_id: 'cat-1',
    title: 'Welcome to the AegisCMS Community!',
    author_id: 'admin',
    author_name: 'Admin',
    is_pinned: true,
    is_locked: false,
    post_count: 5,
    last_post_at: '2026-10-25T12:00:00Z',
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-25T12:00:00Z',
  },
  {
    id: 'th-2',
    category_id: 'cat-2',
    title: 'How to write custom Wasm plugins in Rust',
    author_id: 'user-1',
    author_name: 'Alice',
    is_pinned: false,
    is_locked: false,
    post_count: 12,
    last_post_at: '2026-10-25T14:30:00Z',
    created_at: '2026-10-22T08:00:00Z',
    updated_at: '2026-10-25T14:30:00Z',
  },
];

const mockPosts: ForumPost[] = [
  {
    id: 'p-1',
    thread_id: 'th-1',
    post_number: 1,
    author_id: 'admin',
    author_name: 'Admin',
    body: 'Welcome to the forum! Please adhere to our guidelines.',
    reply_to_post_id: null,
    reply_to_number: null,
    is_deleted: false,
    ip: '127.0.0.1',
    user_agent: 'Mozilla/5.0',
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-20T10:00:00Z',
  },
  {
    id: 'p-2',
    thread_id: 'th-1',
    post_number: 2,
    author_id: 'user-2',
    author_name: 'Bob',
    body: 'Excited to be here! Looking forward to testing out AegisCMS.',
    reply_to_post_id: null,
    reply_to_number: 1,
    is_deleted: false,
    ip: '192.168.1.1',
    user_agent: 'Mozilla/5.0',
    created_at: '2026-10-21T11:00:00Z',
    updated_at: '2026-10-21T11:00:00Z',
  },
];

const forumApi = () => {
  let categories = structuredClone(mockCategories);
  let threads = structuredClone(mockThreads);
  let posts = structuredClone(mockPosts);

  return mockApi(
    ({ method, path, body }) => {
      // 1. Categories
      if (path === '/forum/categories') {
        if (method === 'GET') return { categories };
        if (method === 'POST') {
          const newCat: ForumCategory = {
            id: `cat-${Date.now()}`,
            slug: (body as { slug: string }).slug,
            name: (body as { name: string }).name,
            description: (body as { description: string }).description,
            position: (body as { position: number }).position,
            is_private: (body as { is_private: boolean }).is_private,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          };
          categories.push(newCat);
          return { category: newCat };
        }
      }

      // 2. Threads list
      if (path === '/forum/threads' && method === 'GET') {
        return { threads };
      }

      // 3. Pin thread
      const pinMatch = path.match(/^\/forum\/threads\/([^/]+)\/pin$/);
      if (pinMatch && method === 'POST') {
        const id = decodeURIComponent(pinMatch[1]);
        const pinned = (body as { pinned: boolean }).pinned;
        threads = threads.map((t) => (t.id === id ? { ...t, is_pinned: pinned } : t));
        return { success: true, pinned };
      }

      // 4. Lock thread
      const lockMatch = path.match(/^\/forum\/threads\/([^/]+)\/lock$/);
      if (lockMatch && method === 'POST') {
        const id = decodeURIComponent(lockMatch[1]);
        const locked = (body as { locked: boolean }).locked;
        threads = threads.map((t) => (t.id === id ? { ...t, is_locked: locked } : t));
        return { success: true, locked };
      }

      // 5. Delete thread
      const threadMatch = path.match(/^\/forum\/threads\/([^/]+)$/);
      if (threadMatch && method === 'DELETE') {
        const id = decodeURIComponent(threadMatch[1]);
        threads = threads.filter((t) => t.id !== id);
        return { success: true };
      }

      // 6. Posts list
      const postsMatch = path.match(/^\/forum\/threads\/([^/]+)\/posts$/);
      if (postsMatch && method === 'GET') {
        return { posts };
      }

      // 7. Delete post
      const postMatch = path.match(/^\/forum\/posts\/([^/]+)$/);
      if (postMatch && method === 'DELETE') {
        const id = decodeURIComponent(postMatch[1]);
        posts = posts.map((p) =>
          p.id === id ? { ...p, is_deleted: true, body: '[Deleted by moderator]' } : p
        );
        return { success: true };
      }

      return undefined;
    },
    { delayMs: 10 }
  );
};

const meta = {
  title: 'Forum/ForumManager',
  component: ForumManager,
  parameters: {
    layout: 'fullscreen',
  },
  args: {
    isAdmin: true,
    onShowToast: fn(),
  },
} satisfies Meta<typeof ForumManager>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  beforeEach: () => {
    return forumApi();
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    // Verify title and thread rows loaded
    await waitFor(() => {
      expect(canvas.getByText('Welcome to the AegisCMS Community!')).toBeInTheDocument();
      expect(canvas.getByText('How to write custom Wasm plugins in Rust')).toBeInTheDocument();
    });

    // Check thread count badge
    expect(canvas.getByText(/2 threads/i)).toBeInTheDocument();
  },
};

export const SwitchToCategories: Story = {
  beforeEach: () => {
    return forumApi();
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await waitFor(() => {
      expect(canvas.getByText('Welcome to the AegisCMS Community!')).toBeInTheDocument();
    });

    // Switch to Categories tab
    const categoriesTab = canvas.getByRole('button', { name: /Categories/i });
    await userEvent.click(categoriesTab);

    // Verify categories table is rendered
    await waitFor(() => {
      expect(canvas.getByText('General Discussion')).toBeInTheDocument();
      expect(canvas.getByText('Tech & Architecture')).toBeInTheDocument();
    });
  },
};

export const EditorCategoriesReadOnly: Story = {
  args: { isAdmin: false },
  beforeEach: () => {
    return forumApi();
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await userEvent.click(canvas.getByRole('button', { name: /Categories/i }));

    // Editors see the categories but cannot create, edit or delete them
    await waitFor(() => {
      expect(canvas.getByText('General Discussion')).toBeInTheDocument();
    });
    expect(canvas.getByText(/Requires the admin role|admin ロールが必要です/i)).toBeInTheDocument();
    expect(canvas.queryByRole('button', { name: /New Category|カテゴリを追加/i })).toBeNull();
    expect(canvas.queryAllByTitle(/Edit Category|カテゴリを編集/i)).toHaveLength(0);
  },
};

export const PinAndLockInteraction: Story = {
  beforeEach: () => {
    return forumApi();
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    await waitFor(() => {
      expect(canvas.getByText('How to write custom Wasm plugins in Rust')).toBeInTheDocument();
    });

    // Find the pin button for the second thread (currently unpinned)
    const pinButtons = canvas.getAllByTitle(/^Pin Thread$/i);
    expect(pinButtons.length).toBeGreaterThan(0);
    await userEvent.click(pinButtons[0]);

    // Verify toast is triggered
    await waitFor(() => {
      expect(args.onShowToast).toHaveBeenCalledWith(
        expect.stringMatching(/Thread pinned to top|スレッドを上部にピン留めしました/i),
        'success'
      );
    });
  },
};

export const ViewPostsModal: Story = {
  beforeEach: () => {
    return forumApi();
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await waitFor(() => {
      expect(canvas.getByText('Welcome to the AegisCMS Community!')).toBeInTheDocument();
    });

    // Click on "View & Moderate Posts"
    const viewButtons = canvas.getAllByTitle(/View & Moderate Posts/i);
    await userEvent.click(viewButtons[0]);

    // Modal should open and show posts
    const body = within(document.body);
    await waitFor(() => {
      expect(
        body.getByText(/Welcome to the forum! Please adhere to our guidelines./i)
      ).toBeInTheDocument();
      expect(
        body.getByText(/Excited to be here! Looking forward to testing out AegisCMS./i)
      ).toBeInTheDocument();
    });
  },
};
