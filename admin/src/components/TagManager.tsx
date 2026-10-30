import React, { useEffect, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, Pencil, Search, Tag as TagIcon, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { Tag } from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import styles from './TagManager.module.css';

const PAGE_SIZE = 25;
/** Wait this long after the last key press before searching */
const SEARCH_DELAY_MS = 300;

interface TagManagerProps {
  /** False for viewers: changing tags needs the editor role */
  canEdit: boolean;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

/** Tags of the site: find, create, rename and delete (a tag can be put on any content) */
export const TagManager: React.FC<TagManagerProps> = ({ canEdit, onShowToast }) => {
  const { t } = useTranslation();
  const [tags, setTags] = useState<Tag[] | null>(null);
  const [total, setTotal] = useState<number>(0);
  const [page, setPage] = useState<number>(1);
  const [draft, setDraft] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  // The form creates a tag, or changes the one being edited
  const [editing, setEditing] = useState<Tag | null>(null);
  const [name, setName] = useState<string>('');
  const [slug, setSlug] = useState<string>('');
  const [description, setDescription] = useState<string>('');
  const [saving, setSaving] = useState<boolean>(false);
  const [formError, setFormError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (draft.trim() === query) return;
    const timer = setTimeout(() => {
      setQuery(draft.trim());
      setPage(1);
    }, SEARCH_DELAY_MS);
    return () => clearTimeout(timer);
  }, [draft, query]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listTags({ q: query, limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE })
      .then((result) => {
        if (cancelled) return;
        const lastPage = Math.max(1, Math.ceil(result.total / PAGE_SIZE));
        if (page > lastPage) {
          // The page is gone (its last tag was deleted): show the last page instead
          setPage(lastPage);
          return;
        }
        setTags(result.tags);
        setTotal(result.total);
        setError(null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(messageOf(err, t('tags.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [query, page, reloadCount, t]);

  const resetForm = () => {
    setEditing(null);
    setName('');
    setSlug('');
    setDescription('');
    setFormError(null);
  };

  const startEdit = (tag: Tag) => {
    setEditing(tag);
    setName(tag.name);
    setSlug(tag.slug);
    setDescription(tag.description ?? '');
    setFormError(null);
    nameRef.current?.focus();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setFormError(t('tags.nameRequired'));
      return;
    }
    setSaving(true);
    setFormError(null);
    try {
      const payload = {
        name: name.trim(),
        slug: slug.trim() || undefined,
        description: description.trim() || undefined,
      };
      if (editing) {
        const saved = await aegisApi.updateTag(editing.id, payload);
        onShowToast(t('tags.updated', { name: saved.name }));
      } else {
        const saved = await aegisApi.createTag(payload);
        // The newest tag is not necessarily on this page: show it by searching for its name
        setDraft(saved.name);
        setQuery(saved.name);
        setPage(1);
        onShowToast(t('tags.created', { name: saved.name }));
      }
      resetForm();
      setReloadCount((n) => n + 1);
    } catch (err) {
      setFormError(messageOf(err, t('tags.saveFailed')));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (tag: Tag) => {
    if (!window.confirm(t('tags.confirmDelete', { name: tag.name, count: tag.usage ?? 0 }))) {
      return;
    }
    try {
      await aegisApi.deleteTag(tag.id);
      if (editing?.id === tag.id) resetForm();
      setReloadCount((n) => n + 1);
      onShowToast(t('tags.deleted', { name: tag.name }));
    } catch (err) {
      onShowToast(messageOf(err, t('tags.deleteFailed')), 'error');
    }
  };

  const lastPage = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const from = tags && tags.length > 0 ? (page - 1) * PAGE_SIZE + 1 : 0;
  const to = tags && tags.length > 0 ? from + tags.length - 1 : 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <TagIcon size={22} />
          <h1>{t('tags.title')}</h1>
          {tags && <span className={styles.count}>{total}</span>}
        </div>
        <p className={styles.description}>{t('tags.description')}</p>
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        <div className={styles.search} role="search">
          <Search size={14} className={styles.searchIcon} aria-hidden="true" />
          <input
            type="search"
            value={draft}
            maxLength={100}
            placeholder={t('tags.search')}
            aria-label={t('tags.searchAria')}
            onChange={(e) => setDraft(e.target.value)}
          />
        </div>

        {tags && (
          <div className={styles.list}>
            {tags.map((tag) => (
              <div key={tag.id} className={styles.row}>
                <div className={styles.rowMain}>
                  <div className={styles.rowName}>{tag.name}</div>
                  {tag.description && (
                    <div className={styles.rowDescription}>{tag.description}</div>
                  )}
                </div>
                <span className={styles.rowSlug}>/{tag.slug}</span>
                <span className={styles.rowUsage}>
                  {t('tags.usage', { count: tag.usage ?? 0 })}
                </span>
                {canEdit && (
                  <div className={styles.rowActions}>
                    <Button
                      variant="icon"
                      aria-label={t('tags.editAria', { name: tag.name })}
                      title={t('common.edit')}
                      onClick={() => startEdit(tag)}
                    >
                      <Pencil size={14} />
                    </Button>
                    <Button
                      variant="icon"
                      danger
                      aria-label={t('tags.deleteAria', { name: tag.name })}
                      title={t('common.delete')}
                      onClick={() => void handleDelete(tag)}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                )}
              </div>
            ))}
            {tags.length === 0 && (
              <div className={styles.empty}>{query ? t('tags.noMatch') : t('tags.empty')}</div>
            )}
          </div>
        )}

        {total > PAGE_SIZE && (
          <nav className={styles.pagination} aria-label={t('tags.pagination')}>
            <Button
              variant="icon"
              aria-label={t('tags.previousPage')}
              disabled={page <= 1}
              onClick={() => setPage(page - 1)}
            >
              <ChevronLeft size={16} />
            </Button>
            <span aria-live="polite">{t('tags.range', { from, to, total })}</span>
            <Button
              variant="icon"
              aria-label={t('tags.nextPage')}
              disabled={page >= lastPage}
              onClick={() => setPage(page + 1)}
            >
              <ChevronRight size={16} />
            </Button>
          </nav>
        )}

        {canEdit ? (
          <form className={styles.form} onSubmit={handleSubmit}>
            <div className={styles.formTitle}>
              {editing ? t('tags.editTitle', { name: editing.name }) : t('tags.new')}
            </div>
            <FieldRow>
              <Field label={t('tags.fieldName')}>
                <input
                  ref={nameRef}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder={t('tags.namePlaceholder')}
                  maxLength={50}
                  aria-label={t('tags.nameAria')}
                />
              </Field>
              <Field label={t('tags.fieldSlug')}>
                <input
                  value={slug}
                  onChange={(e) => setSlug(e.target.value)}
                  placeholder={editing ? '' : t('tags.slugPlaceholder')}
                  maxLength={64}
                  aria-label={t('tags.slugAria')}
                />
              </Field>
            </FieldRow>
            <Field label={t('tags.fieldDescription')}>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                maxLength={500}
                aria-label={t('tags.descriptionAria')}
              />
            </Field>
            {formError && <div className={cx(styles.notice, styles.error)}>{formError}</div>}
            <div className={styles.formActions}>
              {editing && (
                <Button type="button" onClick={resetForm}>
                  {t('common.cancel')}
                </Button>
              )}
              <Button type="submit" variant="primary" disabled={saving}>
                {editing ? t('tags.save') : t('tags.create')}
              </Button>
            </div>
          </form>
        ) : (
          <div className={styles.notice}>{t('tags.readOnly')}</div>
        )}
      </div>
    </main>
  );
};
