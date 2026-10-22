import React, { useState } from 'react';
import { Plus, FileText } from 'lucide-react';
import type { ContentItem } from '../types';
import { categoryColor, categoryIdOf } from '../utils/category';

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
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">
          {contentType}s (
          {filter === ALL ? contents.length : `${visible.length}/${contents.length}`})
        </span>
        <button className="tool-btn" onClick={onNew} title="Create new content">
          <Plus size={14} />
          <span>New</span>
        </button>
      </div>

      <div className="sidebar-filter">
        <div className="input-field">
          <span className="input-prefix">category:</span>
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
        </div>
      </div>

      <div className="content-list">
        {visible.map((item) => {
          const category = categoryOf(item);
          return (
            <div
              key={item.id}
              className={`content-card ${selectedId === item.id ? 'active' : ''}`}
              onClick={() => onSelect(item)}
            >
              <div className="content-card-title">{item.title || 'Untitled'}</div>
              {category && (
                <span className="badge-category" title={`Category: ${category.title}`}>
                  <span
                    className="category-swatch"
                    style={{ background: category.color ?? 'var(--border-medium)' }}
                  />
                  {category.title}
                </span>
              )}
              <div className="content-card-meta">
                <span>/{item.slug}</span>
                <span className={`badge-status ${item.status}`}>{item.status}</span>
              </div>
            </div>
          );
        })}

        {visible.length === 0 && (
          <div style={{ textAlign: 'center', padding: '32px 16px', color: 'var(--text-muted)' }}>
            <FileText size={32} style={{ margin: '0 auto 8px', opacity: 0.5 }} />
            {contents.length === 0 ? (
              <p style={{ fontSize: '0.85rem' }}>
                No articles yet.
                <br />
                Click "+ New" to create one.
              </p>
            ) : (
              <p style={{ fontSize: '0.85rem' }}>No articles match this category.</p>
            )}
          </div>
        )}
      </div>
    </aside>
  );
};
