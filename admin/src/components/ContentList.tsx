import React, { useState } from 'react';
import { Plus, FileText } from 'lucide-react';
import type { ContentItem } from '../types';
import { categoryColor, categoryIdOf } from '../utils/category';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './ContentList.module.css';

const STATUS_BADGE: Record<string, string | undefined> = {
  published: styles.published,
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
    return { title: resolved?.title ?? 'Unknown category' };
  };

  return (
    <aside className={styles.sidebar}>
      <div className={styles.header}>
        <span className={styles.title}>
          {contentType}s (
          {filter === ALL ? contents.length : `${visible.length}/${contents.length}`})
        </span>
        <Button variant="tool" onClick={onNew} title="Create new content">
          <Plus size={14} />
          <span>New</span>
        </Button>
      </div>

      <div className={styles.filter}>
        <Field label="category:" className={styles.filterField}>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            aria-label="Filter by category"
          >
            <option value={ALL}>All</option>
            <option value={UNCATEGORIZED}>Uncategorized</option>
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
            <div
              key={item.id}
              className={cx(styles.card, selectedId === item.id && styles.active)}
              onClick={() => onSelect(item)}
            >
              <div className={styles.cardTitle}>{item.title || 'Untitled'}</div>
              {category && (
                <span className={styles.categoryBadge} title={`Category: ${category.title}`}>
                  <span
                    className={styles.swatch}
                    style={{ background: category.color ?? 'var(--border-medium)' }}
                  />
                  {category.title}
                </span>
              )}
              <div className={styles.cardMeta}>
                <span>/{item.slug}</span>
                <span className={cx(styles.status, STATUS_BADGE[item.status])}>{item.status}</span>
              </div>
            </div>
          );
        })}

        {visible.length === 0 && (
          <div className={styles.empty}>
            <FileText size={32} className={styles.emptyIcon} />
            {contents.length === 0 ? (
              <p>
                No articles yet.
                <br />
                Click "+ New" to create one.
              </p>
            ) : (
              <p>No articles match this category.</p>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
