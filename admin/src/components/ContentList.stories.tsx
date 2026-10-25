import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { ContentItem } from '../types';
import { ContentList } from './ContentList';

const mockCategories: ContentItem[] = [
  {
    id: 'cat-1',
    content_type: 'category',
    title: 'Technology',
    slug: 'tech',
    status: 'published',
    blocks: [],
    rendered_html: '',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  },
  {
    id: 'cat-2',
    content_type: 'category',
    title: 'Design',
    slug: 'design',
    status: 'published',
    blocks: [],
    rendered_html: '',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  },
];

const mockArticles: ContentItem[] = [
  {
    id: 'art-1',
    content_type: 'article',
    title: 'Next-Gen Streaming HTML Pipeline',
    slug: 'next-gen-streaming-html',
    status: 'published',
    blocks: [],
    rendered_html: '',
    published_at: '2026-10-20T10:00:00Z',
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-20T10:00:00Z',
    fields: { category_id: 'cat-1' },
    relations: {
      category_id: {
        id: 'cat-1',
        content_type: 'category',
        title: 'Technology',
        slug: 'tech',
      },
    },
  },
  {
    id: 'art-2',
    content_type: 'article',
    title: 'Drafting Modern UI Components',
    slug: 'drafting-modern-ui',
    status: 'draft',
    blocks: [],
    rendered_html: '',
    created_at: '2026-10-22T14:30:00Z',
    updated_at: '2026-10-22T14:30:00Z',
    fields: { category_id: 'cat-2' },
    relations: {
      category_id: {
        id: 'cat-2',
        content_type: 'category',
        title: 'Design',
        slug: 'design',
      },
    },
  },
];

const mockPages: ContentItem[] = [
  {
    id: 'page-1',
    content_type: 'page',
    title: 'About Company',
    slug: 'about',
    status: 'published',
    blocks: [],
    rendered_html: '',
    published_at: '2026-10-15T09:00:00Z',
    created_at: '2026-10-15T09:00:00Z',
    updated_at: '2026-10-15T09:00:00Z',
    fields: { layout: 'default' },
  },
  {
    id: 'page-2',
    content_type: 'page',
    title: 'Summer Campaign Special LP',
    slug: 'summer-campaign',
    status: 'published',
    blocks: [],
    rendered_html: '',
    published_at: '2026-10-25T12:00:00Z',
    created_at: '2026-10-25T12:00:00Z',
    updated_at: '2026-10-25T12:00:00Z',
    fields: { layout: 'blank' },
  },
];

const meta = {
  title: 'Content/ContentList',
  component: ContentList,
  parameters: { layout: 'centered' },
  args: {
    categories: mockCategories,
    selectedId: null,
    onSelect: fn(),
    onNew: fn(),
    onContentTypeChange: fn(),
  },
} satisfies Meta<typeof ContentList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Articles: Story = {
  args: {
    contents: mockArticles,
    contentType: 'article',
    selectedId: 'art-1',
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // 記事タブがアクティブであることを確認
    const articlesTab = canvas.getByRole('tab', { name: /articles|記事/i });
    await expect(articlesTab).toBeVisible();

    // 記事タイトルが表示されていること
    await expect(canvas.getByText('Next-Gen Streaming HTML Pipeline')).toBeVisible();
    await expect(canvas.getByText('Drafting Modern UI Components')).toBeVisible();

    // 記事クリックで onSelect が呼ばれること
    const secondArticle = canvas.getByText('Drafting Modern UI Components');
    await userEvent.click(secondArticle);
    await expect(args.onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'art-2', title: 'Drafting Modern UI Components' })
    );

    // 新規作成ボタンで onNew が呼ばれること
    const newBtn = canvas.getByRole('button', { name: /new|新規/i });
    await userEvent.click(newBtn);
    await expect(args.onNew).toHaveBeenCalled();
  },
};

export const Pages: Story = {
  args: {
    contents: mockPages,
    contentType: 'page',
    selectedId: 'page-2',
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // 固定ページ一覧にタイトルが表示されていること
    await expect(canvas.getByText('About Company')).toBeVisible();
    await expect(canvas.getByText('Summer Campaign Special LP')).toBeVisible();

    // レイアウトバッジ（テーマ標準 / ブランクLP）が表示されていること
    await expect(canvas.getByText(/theme|テーマ標準/i)).toBeVisible();
    await expect(canvas.getByText(/blank lp|ブランクlp/i)).toBeVisible();

    // クリックで onSelect が呼ばれること
    const firstPage = canvas.getByText('About Company');
    await userEvent.click(firstPage);
    await expect(args.onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'page-1', title: 'About Company' })
    );
  },
};

export const TabSwitching: Story = {
  args: {
    contents: mockArticles,
    contentType: 'article',
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // Pages タブをクリックして onContentTypeChange('page') が発火すること
    const pagesTab = canvas.getByRole('tab', { name: /pages|固定ページ/i });
    await userEvent.click(pagesTab);
    await expect(args.onContentTypeChange).toHaveBeenCalledWith('page');
  },
};
