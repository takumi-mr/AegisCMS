import type { Meta, StoryObj } from '@storybook/react-vite';
import { useState, type ComponentProps } from 'react';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import type { ContentSummary, Tag } from '../types';
import { NO_FILTERS } from '../utils/contentFilters';
import { ContentList } from './ContentList';

const summary = (item: Partial<ContentSummary> & Pick<ContentSummary, 'id' | 'title' | 'slug'>) =>
  ({
    content_type: 'article',
    status: 'published',
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-20T10:00:00Z',
    ...item,
  }) as ContentSummary;

const mockCategories: ContentSummary[] = [
  summary({ id: 'cat-1', content_type: 'category', title: 'Technology', slug: 'tech' }),
  summary({ id: 'cat-2', content_type: 'category', title: 'Design', slug: 'design' }),
];

const mockTags: Tag[] = [
  {
    id: 'tag-1',
    slug: 'rust',
    name: 'Rust',
    description: null,
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-20T10:00:00Z',
  },
  {
    id: 'tag-2',
    slug: 'web',
    name: 'Web',
    description: null,
    created_at: '2026-10-20T10:00:00Z',
    updated_at: '2026-10-20T10:00:00Z',
  },
];

const mockArticles: ContentSummary[] = [
  summary({
    id: 'art-1',
    title: 'Next-Gen Streaming HTML Pipeline',
    slug: 'next-gen-streaming-html',
    excerpt: 'How the streaming rewriter keeps pages fast.',
    published_at: '2026-10-20T10:00:00Z',
    fields: { category_id: 'cat-1' },
    relations: {
      category_id: { id: 'cat-1', content_type: 'category', title: 'Technology', slug: 'tech' },
    },
  }),
  summary({
    id: 'art-2',
    title: 'Drafting Modern UI Components',
    slug: 'drafting-modern-ui',
    status: 'draft',
    created_at: '2026-10-22T14:30:00Z',
    updated_at: '2026-10-22T14:30:00Z',
    fields: { category_id: 'cat-2' },
    relations: {
      category_id: { id: 'cat-2', content_type: 'category', title: 'Design', slug: 'design' },
    },
  }),
];

const mockPages: ContentSummary[] = [
  summary({
    id: 'page-1',
    content_type: 'page',
    title: 'About Company',
    slug: 'about',
    published_at: '2026-10-15T09:00:00Z',
    fields: { layout: 'default' },
  }),
  summary({
    id: 'page-2',
    content_type: 'page',
    title: 'Summer Campaign Special LP',
    slug: 'summer-campaign',
    published_at: '2026-10-25T12:00:00Z',
    fields: { layout: 'blank' },
  }),
];

/** Keeps the filters like the app does: the list shows what it last sent up */
const StatefulContentList = (props: ComponentProps<typeof ContentList>) => {
  const [filters, setFilters] = useState(props.filters);
  return (
    <ContentList
      {...props}
      filters={filters}
      onFiltersChange={(next) => {
        props.onFiltersChange(next);
        setFilters(next);
      }}
    />
  );
};

const meta = {
  title: 'Content/ContentList',
  component: ContentList,
  parameters: { layout: 'centered' },
  args: {
    categories: mockCategories,
    tags: mockTags,
    total: mockArticles.length,
    page: 1,
    pageSize: 20,
    filters: NO_FILTERS,
    onFiltersChange: fn(),
    onPageChange: fn(),
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

    // 記事タイトルと要約が表示されていること
    await expect(canvas.getByText('Next-Gen Streaming HTML Pipeline')).toBeVisible();
    await expect(canvas.getByText('Drafting Modern UI Components')).toBeVisible();
    await expect(canvas.getByText('How the streaming rewriter keeps pages fast.')).toBeVisible();

    // 件数は一覧の総件数
    await expect(canvas.getByText(/\(2\)/)).toBeVisible();

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

    // 1 ページに収まるときはページ送りを出さない
    await expect(canvas.queryByRole('navigation')).toBeNull();
  },
};

export const Pages: Story = {
  args: {
    contents: mockPages,
    total: mockPages.length,
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

    // 固定ページにはカテゴリの絞り込みが無く、状態の絞り込みだけがある
    await expect(canvas.queryByRole('combobox', { name: /category|カテゴリ/i })).toBeNull();
    await expect(canvas.getByRole('combobox', { name: /status|状態/i })).toBeVisible();

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

export const Search: Story = {
  render: (args) => <StatefulContentList {...args} />,
  args: {
    contents: mockArticles,
    contentType: 'article',
    filters: { ...NO_FILTERS, category: 'cat-1' },
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const box = canvas.getByRole('searchbox', { name: /search content|検索/i });

    // 入力が止まってから、ほかの絞り込みを保ったまま 1 回だけ検索する
    await userEvent.type(box, 'rust');
    await expect(args.onFiltersChange).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(args.onFiltersChange).toHaveBeenCalledWith({
        q: 'rust',
        category: 'cat-1',
        tag: '',
        status: '',
      })
    );
    await expect(args.onFiltersChange).toHaveBeenCalledTimes(1);

    // Enter ですぐ検索する
    await userEvent.type(box, ' guide{Enter}');
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: 'rust guide',
      category: 'cat-1',
      tag: '',
      status: '',
    });

    // クリアボタンで検索語を消す
    await userEvent.click(canvas.getByRole('button', { name: /clear search|検索をクリア/i }));
    await expect(box).toHaveValue('');
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: '',
      category: 'cat-1',
      tag: '',
      status: '',
    });
  },
};

export const Filters: Story = {
  args: {
    contents: mockArticles,
    contentType: 'article',
    filters: { ...NO_FILTERS, q: 'ui' },
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    // 検索ボックスは今の検索語を表示する
    await expect(canvas.getByRole('searchbox')).toHaveValue('ui');

    await userEvent.selectOptions(
      canvas.getByRole('combobox', { name: /filter by category|カテゴリで絞り込む/i }),
      'cat-2'
    );
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: 'ui',
      category: 'cat-2',
      tag: '',
      status: '',
    });

    // 未分類はサーバーの `none`
    await userEvent.selectOptions(
      canvas.getByRole('combobox', { name: /filter by category|カテゴリで絞り込む/i }),
      'none'
    );
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: 'ui',
      category: 'none',
      tag: '',
      status: '',
    });

    await userEvent.selectOptions(
      canvas.getByRole('combobox', { name: /filter by tag|タグで絞り込む/i }),
      'rust'
    );
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: 'ui',
      category: '',
      tag: 'rust',
      status: '',
    });

    await userEvent.selectOptions(
      canvas.getByRole('combobox', { name: /filter by status|状態で絞り込む/i }),
      'draft'
    );
    await expect(args.onFiltersChange).toHaveBeenLastCalledWith({
      q: 'ui',
      category: '',
      tag: '',
      status: 'draft',
    });
  },
};

export const Pagination: Story = {
  args: {
    contents: mockArticles,
    contentType: 'article',
    total: 45,
    page: 2,
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // 総件数・表示している範囲
    await expect(canvas.getByText(/\(45\)/)).toBeVisible();
    await expect(canvas.getByText(/21.40|45/, { selector: '[aria-live]' })).toBeVisible();

    await userEvent.click(canvas.getByRole('button', { name: /previous page|前のページ/i }));
    await expect(args.onPageChange).toHaveBeenLastCalledWith(1);
    await userEvent.click(canvas.getByRole('button', { name: /next page|次のページ/i }));
    await expect(args.onPageChange).toHaveBeenLastCalledWith(3);
  },
};

export const FirstAndLastPage: Story = {
  args: {
    contents: mockArticles,
    contentType: 'article',
    total: 45,
    page: 1,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 最初のページでは「前へ」を押せない
    await expect(canvas.getByRole('button', { name: /previous page|前のページ/i })).toBeDisabled();
    await expect(canvas.getByRole('button', { name: /next page|次のページ/i })).toBeEnabled();
  },
};

export const LastPage: Story = {
  args: {
    contents: [mockArticles[0]],
    contentType: 'article',
    total: 41,
    page: 3,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('button', { name: /next page|次のページ/i })).toBeDisabled();
    await expect(canvas.getByRole('button', { name: /previous page|前のページ/i })).toBeEnabled();
  },
};

export const NoMatch: Story = {
  args: {
    contents: [],
    total: 0,
    contentType: 'article',
    filters: { ...NO_FILTERS, q: 'zzz' },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByText(
        /nothing matches your search or filters|検索や絞り込みに一致するものがありません/i
      )
    ).toBeVisible();
  },
};

export const Empty: Story = {
  args: {
    contents: [],
    total: 0,
    contentType: 'article',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText(/no articles yet|まだ記事がありません/i)).toBeVisible();
  },
};

export const Loading: Story = {
  args: {
    contents: [],
    total: 0,
    contentType: 'article',
    loading: true,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // 読み込み中に「記事がありません」とは出さない
    await expect(canvas.getByText(/loading|読み込み中/i)).toBeVisible();
    await expect(canvas.queryByText(/no articles yet|まだ記事がありません/i)).toBeNull();
  },
};
