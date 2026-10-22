import React, { useEffect, useRef, useState } from 'react';
import { Plus, Tags } from 'lucide-react';
import { aegisApi } from '../api/client';
import type { ContentItem } from '../types';
import { categoryColor } from '../utils/category';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './CategoryManager.module.css';

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

  // Focus once on open; the parent re-renders (health checks, toasts) must not steal focus
  useEffect(() => {
    titleRef.current?.focus();
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
    <Modal title="Categories" icon={<Tags size={18} />} onClose={onClose}>
      <div className={styles.list}>
        {categories.map((category) => (
          <div key={category.id} className={styles.row}>
            <span
              className={styles.swatch}
              style={{ background: categoryColor(category) ?? 'var(--border-medium)' }}
            />
            <div className={styles.rowMain}>
              <div className={styles.rowTitle}>{category.title}</div>
              {typeof category.fields?.description === 'string' && category.fields.description && (
                <div className={styles.rowDescription}>{category.fields.description}</div>
              )}
            </div>
            <span className={styles.rowSlug}>/{category.slug}</span>
          </div>
        ))}
        {categories.length === 0 && (
          <div className={styles.empty}>No categories yet. Create the first one below.</div>
        )}
      </div>

      <form className={styles.form} onSubmit={handleSubmit}>
        <div className={styles.formTitle}>New category</div>
        <FieldRow>
          <Field label="title:">
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
          </Field>
          <Field label="slug:">
            <input
              value={slug}
              onChange={(e) => {
                setSlug(e.target.value);
                setSlugEdited(true);
              }}
              placeholder="news"
              aria-label="Category slug"
            />
          </Field>
          <Field as="label" label="color:" className={styles.colorField}>
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              aria-label="Category color"
            />
          </Field>
        </FieldRow>
        <Field label="description:">
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Optional"
            aria-label="Category description"
          />
        </Field>
        {error && <div className={styles.error}>{error}</div>}
        <div className={styles.formActions}>
          <Button type="submit" variant="primary" disabled={isSaving}>
            <Plus size={16} />
            <span>{isSaving ? 'Creating...' : 'Create category'}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
