import React, { useEffect, useRef, useState } from 'react';
import { Plus, Tags, X } from 'lucide-react';
import { aegisApi } from '../api/client';
import type { ContentItem } from '../types';
import { categoryColor } from '../utils/category';

const DEFAULT_COLOR = '#6366f1';

interface CategoryManagerProps {
  categories: ContentItem[];
  /** Called after a category is created so the parent can reload and select it */
  onCreated: (category: ContentItem) => void;
  onClose: () => void;
}

const toSlug = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, '-')
    .replace(/^-|-$/g, '');

export const CategoryManager: React.FC<CategoryManagerProps> = ({
  categories,
  onCreated,
  onClose,
}) => {
  const [title, setTitle] = useState<string>('');
  const [slug, setSlug] = useState<string>('');
  const [slugEdited, setSlugEdited] = useState<boolean>(false);
  const [description, setDescription] = useState<string>('');
  const [color, setColor] = useState<string>(DEFAULT_COLOR);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const titleRef = useRef<HTMLInputElement>(null);

  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  // Focus once on open; the parent re-renders (health checks, toasts) must not steal focus
  useEffect(() => {
    titleRef.current?.focus();
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCloseRef.current();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !slug.trim()) {
      setError('Title and slug are required.');
      return;
    }
    if (categories.some((c) => c.slug === slug.trim())) {
      setError(`A category with the slug "${slug.trim()}" already exists.`);
      return;
    }

    setIsSaving(true);
    setError(null);
    try {
      const created = await aegisApi.createCategory({
        title: title.trim(),
        slug: slug.trim(),
        status: 'published',
        description: description.trim() || null,
        color,
      });
      setTitle('');
      setSlug('');
      setSlugEdited(false);
      setDescription('');
      setColor(DEFAULT_COLOR);
      onCreated(created);
      titleRef.current?.focus();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create category');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="modal-overlay" onMouseDown={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="category-manager-title"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <div className="modal-title">
            <Tags size={18} />
            <h2 id="category-manager-title">Categories</h2>
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            <X size={16} />
          </button>
        </div>

        <div className="category-list">
          {categories.map((category) => (
            <div key={category.id} className="category-row">
              <span
                className="category-swatch"
                style={{ background: categoryColor(category) ?? 'var(--border-medium)' }}
              />
              <div className="category-row-main">
                <div className="category-row-title">{category.title}</div>
                {typeof category.fields?.description === 'string' &&
                  category.fields.description && (
                    <div className="category-row-description">{category.fields.description}</div>
                  )}
              </div>
              <span className="category-row-slug">/{category.slug}</span>
            </div>
          ))}
          {categories.length === 0 && (
            <div className="category-empty">No categories yet. Create the first one below.</div>
          )}
        </div>

        <form className="category-form" onSubmit={handleSubmit}>
          <div className="category-form-title">New category</div>
          <div className="meta-row">
            <div className="input-field">
              <span className="input-prefix">title:</span>
              <input
                ref={titleRef}
                value={title}
                onChange={(e) => {
                  setTitle(e.target.value);
                  if (!slugEdited) setSlug(toSlug(e.target.value));
                }}
                placeholder="News"
                maxLength={100}
                aria-label="Category title"
              />
            </div>
            <div className="input-field">
              <span className="input-prefix">slug:</span>
              <input
                value={slug}
                onChange={(e) => {
                  setSlug(e.target.value);
                  setSlugEdited(true);
                }}
                placeholder="news"
                aria-label="Category slug"
              />
            </div>
            <label className="input-field category-color-field">
              <span className="input-prefix">color:</span>
              <input
                type="color"
                value={color}
                onChange={(e) => setColor(e.target.value)}
                aria-label="Category color"
              />
            </label>
          </div>
          <div className="input-field">
            <span className="input-prefix">description:</span>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Optional"
              aria-label="Category description"
            />
          </div>
          {error && <div className="category-error">{error}</div>}
          <div className="category-form-actions">
            <button type="submit" className="btn btn-primary" disabled={isSaving}>
              <Plus size={16} />
              <span>{isSaving ? 'Creating...' : 'Create category'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
