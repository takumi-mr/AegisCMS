import React, { useEffect, useRef, useState } from 'react';
import { Plus, FileText, Layers, Clock, Search, X, ChevronLeft, ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ContentSummary, Tag } from '../types';
import { categoryColor, categoryIdOf } from '../utils/category';
import { isLabeledStatus } from '../utils/contentStatus';
import { cx } from '../utils/cx';
import {
  isFiltered as hasFilters,
  STATUS_FILTERS,
  UNCATEGORIZED,
  type ContentListFilters,
} from '../utils/contentFilters';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './ContentList.module.css';

const STATUS_BADGE: Record<string, string | undefined> = {
  published: styles.published,
  scheduled: styles.scheduled,
  draft: styles.draft,
};

/** Wait this long after the last key press before searching */
const SEARCH_DELAY_MS = 300;

interface ContentListProps {
  /** The current page of the list (the server searches, filters and pages it) */
  contents: ContentSummary[];
  /** The number of items matching the filters, not only this page */
  total: number;
  /** The current page, starting at 1 */
  page: number;
  pageSize: number;
  /** True while a page is being loaded */
  loading?: boolean;
  filters: ContentListFilters;
  onFiltersChange: (filters: ContentListFilters) => void;
  onPageChange: (page: number) => void;
  categories: ContentSummary[];
  tags?: Tag[];
  selectedId: string | null;
  onSelect: (content: ContentSummary) => void;
  onNew: () => void;
  contentType: string;
  onContentTypeChange?: (type: 'article' | 'page') => void;
}

export const ContentList: React.FC<ContentListProps> = ({
  contents,
  total,
  page,
  pageSize,
  loading = false,
  filters,
  onFiltersChange,
  onPageChange,
  categories,
  tags,
  selectedId,
  onSelect,
  onNew,
  contentType,
  onContentTypeChange,
}) => {
  const { t } = useTranslation();
  const isArticles = contentType === 'article';
  const categoriesById = new Map(categories.map((c) => [c.id, c]));

  // The search box keeps what is typed; the list is searched once typing pauses. `emitted` is the
  // last search sent up, so a slow answer never overwrites newer typing
  const [draft, setDraft] = useState(filters.q);
  const emitted = useRef(filters.q);
  const latestFilters = useRef(filters);
  latestFilters.current = filters;
  useEffect(() => {
    if (filters.q !== emitted.current) {
      emitted.current = filters.q;
      setDraft(filters.q);
    }
  }, [filters.q]);
  useEffect(() => {
    if (draft === emitted.current) return;
    const timer = setTimeout(() => {
      emitted.current = draft;
      onFiltersChange({ ...latestFilters.current, q: draft });
    }, SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);

  const searchNow = (q: string) => {
    emitted.current = q;
    setDraft(q);
    if (q !== latestFilters.current.q) onFiltersChange({ ...latestFilters.current, q });
  };

  const isFiltered = hasFilters(filters, isArticles);
  const lastPage = Math.max(1, Math.ceil(total / pageSize));
  const from = contents.length === 0 ? 0 : (page - 1) * pageSize + 1;
  const to = contents.length === 0 ? 0 : from + contents.length - 1;

  /** Category shown on a card; falls back to the relation resolved by the server */
  const categoryOf = (item: ContentSummary): { title: string; color?: string } | undefined => {
    if (!isArticles) return undefined;
    const id = categoryIdOf(item);
    if (!id) return undefined;
    const category = categoriesById.get(id);
    if (category) return { title: category.title, color: categoryColor(category) };
    const resolved = item.relations?.category_id;
    return { title: resolved?.title ?? t('contentList.unknownCategory') };
  };

  const getLayoutLabel = (item: ContentSummary): string => {
    const raw = item.fields?.layout ?? (item as unknown as { layout?: string }).layout;
    if (raw === 'blank') return t('editor.layoutBlankShort');
    return t('editor.layoutDefaultShort');
  };

  return (
    <aside className={styles.sidebar}>
      {onContentTypeChange && (
        <div
          className={styles.typeSelector}
          role="tablist"
          aria-label={t('contentList.typeSelector')}
        >
          <button
            type="button"
            role="tab"
            aria-selected={isArticles}
            className={cx(styles.typeButton, isArticles && styles.activeType)}
            onClick={() => onContentTypeChange('article')}
          >
            <FileText size={13} />
            <span>{t('contentList.articles')}</span>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={!isArticles}
            className={cx(styles.typeButton, !isArticles && styles.activeType)}
            onClick={() => onContentTypeChange('page')}
          >
            <Layers size={13} />
            <span>{t('contentList.pages')}</span>
          </button>
        </div>
      )}

      <div className={styles.header}>
        <span className={styles.title}>
          {t('contentList.title', {
            type: isArticles ? t('contentList.articles') : t('contentList.pages'),
            count: total,
          })}
        </span>
        <Button variant="tool" onClick={onNew} title={t('contentList.newTitle')}>
          <Plus size={14} />
          <span>{t('contentList.new')}</span>
        </Button>
      </div>

      <div className={styles.search} role="search">
        <Search size={14} className={styles.searchIcon} aria-hidden="true" />
        <input
          type="search"
          value={draft}
          maxLength={100}
          placeholder={t('contentList.search')}
          aria-label={t('contentList.searchAria')}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') searchNow(draft);
          }}
        />
        {draft !== '' && (
          <button
            type="button"
            className={styles.clearSearch}
            aria-label={t('contentList.clearSearch')}
            onClick={() => searchNow('')}
          >
            <X size={13} />
          </button>
        )}
      </div>

      <div className={styles.filter}>
        {isArticles && (
          <Field label={t('contentList.category')} className={styles.filterField}>
            <select
              value={filters.category}
              onChange={(e) => onFiltersChange({ ...filters, category: e.target.value })}
              aria-label={t('contentList.filterAria')}
            >
              <option value="">{t('contentList.all')}</option>
              <option value={UNCATEGORIZED}>{t('contentList.uncategorized')}</option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.title}
                </option>
              ))}
            </select>
          </Field>
        )}
        {tags && tags.length > 0 && (
          <Field label={t('contentList.tag')} className={styles.filterField}>
            <select
              value={filters.tag}
              onChange={(e) => onFiltersChange({ ...filters, tag: e.target.value })}
              aria-label={t('contentList.tagAria')}
            >
              <option value="">{t('contentList.all')}</option>
              {tags.map((tag) => (
                <option key={tag.id} value={tag.slug}>
                  {tag.name}
                </option>
              ))}
            </select>
          </Field>
        )}
        <Field label={t('contentList.status')} className={styles.filterField}>
          <select
            value={filters.status}
            onChange={(e) => onFiltersChange({ ...filters, status: e.target.value })}
            aria-label={t('contentList.statusAria')}
          >
            <option value="">{t('contentList.all')}</option>
            {STATUS_FILTERS.map((status) => (
              <option key={status} value={status}>
                {t(`common.status.${status}`)}
              </option>
            ))}
          </select>
        </Field>
      </div>

      <div className={cx(styles.list, loading && styles.loading)} aria-busy={loading}>
        {contents.map((item) => {
          const category = categoryOf(item);
          return (
            <button
              key={item.id}
              type="button"
              className={cx(styles.card, selectedId === item.id && styles.active)}
              aria-current={selectedId === item.id ? 'true' : undefined}
              onClick={() => onSelect(item)}
            >
              <span className={styles.cardTitle}>{item.title || t('contentList.untitled')}</span>
              {category && (
                <span
                  className={styles.categoryBadge}
                  title={t('contentList.categoryTitle', { title: category.title })}
                >
                  <span
                    className={styles.swatch}
                    style={{ background: category.color ?? 'var(--border-medium)' }}
                  />
                  {category.title}
                </span>
              )}
              {!isArticles && <span className={styles.layoutBadge}>{getLayoutLabel(item)}</span>}
              {item.excerpt && <span className={styles.excerpt}>{item.excerpt}</span>}
              <span className={styles.cardMeta}>
                <span>/{item.slug}</span>
                <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                  {item.unpublish_at && (
                    <span
                      className={styles.unpublishBadge}
                      title={t('contentList.unpublishFor', {
                        date: formatDateTime(item.unpublish_at),
                      })}
                    >
                      <Clock size={10} />
                    </span>
                  )}
                  <span
                    className={cx(styles.status, STATUS_BADGE[item.status])}
                    title={
                      item.status === 'scheduled' && item.published_at
                        ? t('contentList.scheduledFor', { date: formatDateTime(item.published_at) })
                        : undefined
                    }
                  >
                    {isLabeledStatus(item.status) ? t(`common.status.${item.status}`) : item.status}
                  </span>
                </span>
              </span>
            </button>
          );
        })}

        {contents.length === 0 && (
          <div className={styles.empty}>
            <FileText size={32} className={styles.emptyIcon} />
            {loading ? (
              <p>{t('contentList.loading')}</p>
            ) : isFiltered ? (
              <p>{t('contentList.noMatch')}</p>
            ) : (
              <p>
                {t('contentList.empty')}
                <br />
                {t('contentList.emptyHint')}
              </p>
            )}
          </div>
        )}
      </div>

      {total > pageSize && (
        <nav className={styles.pager} aria-label={t('contentList.pagination')}>
          <button
            type="button"
            className={styles.pagerButton}
            aria-label={t('contentList.previousPage')}
            disabled={page <= 1}
            onClick={() => onPageChange(page - 1)}
          >
            <ChevronLeft size={16} />
          </button>
          <span className={styles.pagerRange} aria-live="polite">
            {t('contentList.range', { from, to, total })}
          </span>
          <button
            type="button"
            className={styles.pagerButton}
            aria-label={t('contentList.nextPage')}
            disabled={page >= lastPage}
            onClick={() => onPageChange(page + 1)}
          >
            <ChevronRight size={16} />
          </button>
        </nav>
      )}
    </aside>
  );
};
