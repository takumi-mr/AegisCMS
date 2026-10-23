import React, { useRef, useState } from 'react';
import { Plus, Tags } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { ContentItem } from '../types';
import { categoryColor } from '../utils/category';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import styles from './CategoryManager.module.css';

const DEFAULT_COLOR = '#6366f1';

interface CategoryManagerProps {
  categories: ContentItem[];
  /** False for viewers: creating categories needs the editor role */
  canCreate: boolean;
  /** Called after a category is created so the parent can add it to its list */
  onCreated: (category: ContentItem) => void;
}

const toSlug = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, '-')
    .replace(/^-|-$/g, '');

export const CategoryManager: React.FC<CategoryManagerProps> = ({
  categories,
  canCreate,
  onCreated,
}) => {
  const { t } = useTranslation();
  const [title, setTitle] = useState<string>('');
  const [slug, setSlug] = useState<string>('');
  const [slugEdited, setSlugEdited] = useState<boolean>(false);
  const [description, setDescription] = useState<string>('');
  const [color, setColor] = useState<string>(DEFAULT_COLOR);
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const titleRef = useRef<HTMLInputElement>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !slug.trim()) {
      setError(t('categories.required'));
      return;
    }
    if (categories.some((c) => c.slug === slug.trim())) {
      setError(t('categories.duplicate', { slug: slug.trim() }));
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
      setError(err instanceof Error ? err.message : t('categories.failed'));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Tags size={22} />
          <h1>{t('categories.title')}</h1>
          <span className={styles.count}>{categories.length}</span>
        </div>
        <p className={styles.description}>{t('categories.description')}</p>

        <div className={styles.list}>
          {categories.map((category) => (
            <div key={category.id} className={styles.row}>
              <span
                className={styles.swatch}
                style={{ background: categoryColor(category) ?? 'var(--border-medium)' }}
              />
              <div className={styles.rowMain}>
                <div className={styles.rowTitle}>{category.title}</div>
                {typeof category.fields?.description === 'string' &&
                  category.fields.description && (
                    <div className={styles.rowDescription}>{category.fields.description}</div>
                  )}
              </div>
              <span className={styles.rowSlug}>/{category.slug}</span>
            </div>
          ))}
          {categories.length === 0 && <div className={styles.empty}>{t('categories.empty')}</div>}
        </div>

        {canCreate ? (
          <form className={styles.form} onSubmit={handleSubmit}>
            <div className={styles.formTitle}>{t('categories.new')}</div>
            <FieldRow>
              <Field label={t('categories.fieldTitle')}>
                <input
                  ref={titleRef}
                  value={title}
                  onChange={(e) => {
                    setTitle(e.target.value);
                    if (!slugEdited) setSlug(toSlug(e.target.value));
                  }}
                  placeholder={t('categories.titlePlaceholder')}
                  maxLength={100}
                  aria-label={t('categories.titleAria')}
                />
              </Field>
              <Field label={t('categories.fieldSlug')}>
                <input
                  value={slug}
                  onChange={(e) => {
                    setSlug(e.target.value);
                    setSlugEdited(true);
                  }}
                  placeholder={t('categories.slugPlaceholder')}
                  aria-label={t('categories.slugAria')}
                />
              </Field>
              <Field as="label" label={t('categories.fieldColor')} className={styles.colorField}>
                <input
                  type="color"
                  value={color}
                  onChange={(e) => setColor(e.target.value)}
                  aria-label={t('categories.colorAria')}
                />
              </Field>
            </FieldRow>
            <Field label={t('categories.fieldDescription')}>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t('categories.optional')}
                aria-label={t('categories.descriptionAria')}
              />
            </Field>
            {error && <div className={styles.error}>{error}</div>}
            <div className={styles.formActions}>
              <Button type="submit" variant="primary" disabled={isSaving}>
                <Plus size={16} />
                <span>{isSaving ? t('categories.creating') : t('categories.create')}</span>
              </Button>
            </div>
          </form>
        ) : (
          <div className={styles.notice}>{t('categories.readOnly')}</div>
        )}
      </div>
    </main>
  );
};
