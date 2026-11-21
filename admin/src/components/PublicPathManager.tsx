import React, { useEffect, useState } from 'react';
import { Link2, RotateCcw, Save } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { PublicPathEntry } from '../types';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './PublicPathManager.module.css';

interface PublicPathManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/**
 * The URL prefix of each content type's public pages (`GET /api/v1/public-paths`,
 * `PUT /api/v1/public-paths/{content_type}`, admin only). A saved value overrides the schema file's
 * `public_path` and applies after the server restarts; an empty field goes back to the file's value.
 */
export const PublicPathManager: React.FC<PublicPathManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [entries, setEntries] = useState<PublicPathEntry[] | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getPublicPaths()
      .then((loaded) => {
        if (cancelled) return;
        setEntries(loaded);
        setDrafts(Object.fromEntries(loaded.map((e) => [e.content_type, e.override ?? ''])));
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : t('publicPaths.loadFailed'));
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const save = async (entry: PublicPathEntry, value: string | null) => {
    setSaving(entry.content_type);
    setError(null);
    try {
      const next = await aegisApi.updatePublicPath(entry.content_type, value);
      setEntries(
        (list) => list?.map((e) => (e.content_type === next.content_type ? next : e)) ?? null
      );
      setDrafts((d) => ({ ...d, [next.content_type]: next.override ?? '' }));
      onShowToast(
        value === null
          ? t('publicPaths.reset', { label: entry.label })
          : t('publicPaths.saved', { label: entry.label })
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : t('publicPaths.saveFailed'));
    } finally {
      setSaving(null);
    }
  };

  const needsRestart = entries?.some((e) => e.restart_required) ?? false;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Link2 size={22} />
          <h1>{t('publicPaths.title')}</h1>
        </div>
        <p className={styles.description}>{t('publicPaths.description')}</p>

        {needsRestart && (
          <div className={styles.notice} role="status">
            {t('publicPaths.restartNotice')}
          </div>
        )}
        {error && (
          <div className={styles.error} role="alert">
            {error}
          </div>
        )}
        {entries?.length === 0 && <p className={styles.hint}>{t('publicPaths.empty')}</p>}

        {entries?.map((entry) => {
          const draft = drafts[entry.content_type] ?? '';
          const isDirty = draft.trim() !== (entry.override ?? '');
          const isSaving = saving === entry.content_type;
          return (
            <form
              key={entry.content_type}
              className={styles.card}
              onSubmit={(e) => {
                e.preventDefault();
                void save(entry, draft.trim() || null);
              }}
            >
              <div className={styles.cardHeader}>
                <span className={styles.label}>{entry.label}</span>
                <code className={styles.type}>{entry.content_type}</code>
              </div>
              <div className={styles.inputRow}>
                <Field className={styles.inputField}>
                  <input
                    value={draft}
                    onChange={(e) =>
                      setDrafts((d) => ({ ...d, [entry.content_type]: e.target.value }))
                    }
                    placeholder={entry.file ?? `/${entry.content_type}`}
                    maxLength={65}
                    aria-label={t('publicPaths.inputAria', { label: entry.label })}
                    disabled={isSaving}
                  />
                </Field>
                <Button type="submit" variant="primary" disabled={!isDirty || isSaving}>
                  <Save size={14} />
                  <span>{isSaving ? t('publicPaths.saving') : t('publicPaths.save')}</span>
                </Button>
                {entry.override !== null && (
                  <Button
                    variant="tool"
                    onClick={() => void save(entry, null)}
                    disabled={isSaving}
                    title={t('publicPaths.resetButton')}
                    aria-label={t('publicPaths.resetAria', { label: entry.label })}
                  >
                    <RotateCcw size={14} />
                  </Button>
                )}
              </div>
              <ul className={styles.status}>
                <li>
                  {t('publicPaths.running', {
                    value: entry.running ?? t('publicPaths.notPublic'),
                  })}
                </li>
                {entry.restart_required && (
                  <li className={styles.pending}>
                    {entry.configured
                      ? t('publicPaths.pending', { value: entry.configured })
                      : t('publicPaths.pendingNone')}
                  </li>
                )}
                <li>
                  {entry.file
                    ? t('publicPaths.fileValue', { value: entry.file })
                    : t('publicPaths.fileNone')}
                </li>
              </ul>
              {entry.content_type === 'article' && (
                <p className={styles.hint}>{t('publicPaths.articleHint')}</p>
              )}
            </form>
          );
        })}

        <p className={styles.hint}>{t('publicPaths.hint')}</p>
      </div>
    </main>
  );
};
