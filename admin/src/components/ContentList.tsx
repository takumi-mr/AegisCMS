import React, { useState } from 'react';
import { Plus, FileText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ContentItem } from '../types';
import { categoryColor, categoryIdOf } from '../utils/category';
import { isLabeledStatus } from '../utils/contentStatus';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './ContentList.module.css';

const STATUS_BADGE: Record<string, string | undefined> = {
  published: styles.published,
  scheduled: styles.scheduled,
  draft: styles.draft,
};

/** Category filter: every item, items without a category, or a category id */
const ALL = '__all__';
const UNCATEGORIZED = '__none__';

interface ContentListProps {
  contents: ContentItem[];
  categories: ContentItem[];
  selectedId: string | null;
  onSelect: (content: ContentItem) => void;
  onNew: () => void;
  contentType: string;
}

export const ContentList: React.FC<ContentListProps> = ({
  contents,
  categories,
  selectedId,
  onSelect,
  onNew,
  contentType,
}) => {
  const { t } = useTranslation();
  const [filter, setFilter] = useState<string>(ALL);

  const categoriesById = new Map(categories.map((c) => [c.id, c]));
  const visible = contents.filter((item) => {
    if (filter === ALL) return true;
    const id = categoryIdOf(item);
    return filter === UNCATEGORIZED ? id === undefined : id === filter;
  });

  /** Category shown on a card; falls back to the relation resolved by the server */
  const categoryOf = (item: ContentItem): { title: string; color?: string } | undefined => {
    const id = categoryIdOf(item);
    if (!id) return undefined;
    const category = categoriesById.get(id);
    if (category) return { title: category.title, color: categoryColor(category) };
    const resolved = item.relations?.category_id;
    return { title: resolved?.title ?? t('contentList.unknownCategory') };
  };

  return (
    <aside className={styles.sidebar}>
      <div className={styles.header}>
        <span className={styles.title}>
          {t('contentList.title', {
            type: contentType === 'article' ? t('contentList.articles') : contentType,
            count: filter === ALL ? contents.length : `${visible.length}/${contents.length}`,
          })}
        </span>
        <Button variant="tool" onClick={onNew} title={t('contentList.newTitle')}>
          <Plus size={14} />
          <span>{t('contentList.new')}</span>
        </Button>
      </div>

      <div className={styles.filter}>
        <Field label={t('contentList.category')} className={styles.filterField}>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            aria-label={t('contentList.filterAria')}
          >
            <option value={ALL}>{t('contentList.all')}</option>
            <option value={UNCATEGORIZED}>{t('contentList.uncategorized')}</option>
            {categories.map((category) => (
              <option key={category.id} value={category.id}>
                {category.title}
              </option>
            ))}
          </select>
        </Field>
      </div>

      <div className={styles.list}>
        {visible.map((item) => {
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
              <span className={styles.cardMeta}>
                <span>/{item.slug}</span>
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
            </button>
          );
        })}

        {visible.length === 0 && (
          <div className={styles.empty}>
            <FileText size={32} className={styles.emptyIcon} />
            {contents.length === 0 ? (
              <p>
                {t('contentList.empty')}
                <br />
                {t('contentList.emptyHint')}
              </p>
            ) : (
              <p>{t('contentList.noMatch')}</p>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
