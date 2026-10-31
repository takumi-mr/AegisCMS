import React, { useEffect, useRef, useState } from 'react';
import {
  ArrowRightLeft,
  ChevronLeft,
  ChevronRight,
  Pencil,
  Search,
  Trash2,
  Wand2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type { Redirect } from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import styles from './RedirectManager.module.css';

const PAGE_SIZE = 25;
/** Wait this long after the last key press before searching */
const SEARCH_DELAY_MS = 300;

interface RedirectManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

/** Redirects of the site (old URL → new URL), including the ones kept when a slug changed */
export const RedirectManager: React.FC<RedirectManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [redirects, setRedirects] = useState<Redirect[] | null>(null);
  const [total, setTotal] = useState<number>(0);
  const [page, setPage] = useState<number>(1);
  const [draft, setDraft] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  // The form adds a redirect, or changes the one being edited
  const [editing, setEditing] = useState<Redirect | null>(null);
  const [from, setFrom] = useState<string>('');
  const [to, setTo] = useState<string>('');
  const [status, setStatus] = useState<301 | 302>(301);
  const [saving, setSaving] = useState<boolean>(false);
  const [formError, setFormError] = useState<string | null>(null);
  const fromRef = useRef<HTMLInputElement>(null);

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
      .listRedirects({ q: query, limit: PAGE_SIZE, offset: (page - 1) * PAGE_SIZE })
      .then((result) => {
        if (cancelled) return;
        const lastPage = Math.max(1, Math.ceil(result.total / PAGE_SIZE));
        if (page > lastPage) {
          // The page is gone (its last redirect was deleted): show the last page instead
          setPage(lastPage);
          return;
        }
        setRedirects(result.redirects);
        setTotal(result.total);
        setError(null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(messageOf(err, t('redirects.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [query, page, reloadCount, t]);

  const resetForm = () => {
    setEditing(null);
    setFrom('');
    setTo('');
    setStatus(301);
    setFormError(null);
  };

  const startEdit = (redirect: Redirect) => {
    setEditing(redirect);
    setFrom(redirect.from);
    setTo(redirect.to);
    setStatus(redirect.status === 302 ? 302 : 301);
    setFormError(null);
    fromRef.current?.focus();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!from.trim() || !to.trim()) {
      setFormError(t('redirects.bothRequired'));
      return;
    }
    setSaving(true);
    setFormError(null);
    try {
      const payload = { from: from.trim(), to: to.trim(), status };
      if (editing) {
        const saved = await aegisApi.updateRedirect(editing.id, payload);
        onShowToast(t('redirects.updated', { from: saved.from }));
      } else {
        const saved = await aegisApi.createRedirect(payload);
        // The newest one is first, but a search may hide it: show it by searching for its path
        setDraft(saved.from);
        setQuery(saved.from);
        setPage(1);
        onShowToast(t('redirects.created', { from: saved.from }));
      }
      resetForm();
      setReloadCount((n) => n + 1);
    } catch (err) {
      // The server's reasons are English; the code lets us show the message in the UI language
      const code = err instanceof ApiError ? err.code : undefined;
      setFormError(
        code === 'duplicate_redirect'
          ? t('redirects.duplicate')
          : code === 'redirect_loop'
            ? t('redirects.loop')
            : code === 'too_many_redirects'
              ? t('redirects.tooMany')
              : messageOf(err, t('redirects.saveFailed'))
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (redirect: Redirect) => {
    if (!window.confirm(t('redirects.confirmDelete', { from: redirect.from }))) return;
    try {
      await aegisApi.deleteRedirect(redirect.id);
      if (editing?.id === redirect.id) resetForm();
      setReloadCount((n) => n + 1);
      onShowToast(t('redirects.deleted', { from: redirect.from }));
    } catch (err) {
      onShowToast(messageOf(err, t('redirects.deleteFailed')), 'error');
    }
  };

  const lastPage = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const first = redirects && redirects.length > 0 ? (page - 1) * PAGE_SIZE + 1 : 0;
  const last = redirects && redirects.length > 0 ? first + redirects.length - 1 : 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <ArrowRightLeft size={22} />
          <h1>{t('redirects.title')}</h1>
          {redirects && <span className={styles.count}>{total}</span>}
        </div>
        <p className={styles.description}>{t('redirects.description')}</p>
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        <div className={styles.search} role="search">
          <Search size={14} className={styles.searchIcon} aria-hidden="true" />
          <input
            type="search"
            value={draft}
            maxLength={100}
            placeholder={t('redirects.search')}
            aria-label={t('redirects.searchAria')}
            onChange={(e) => setDraft(e.target.value)}
          />
        </div>

        {redirects && (
          <div className={styles.list}>
            {redirects.map((redirect) => (
              <div key={redirect.id} className={styles.row}>
                <div className={styles.rowMain}>
                  <div className={styles.rowFrom}>{redirect.from}</div>
                  <div className={styles.rowTo}>
                    <span aria-hidden="true">→ </span>
                    {redirect.to}
                  </div>
                </div>
                {redirect.auto && (
                  <span className={styles.auto} title={t('redirects.autoTitle')}>
                    <Wand2 size={12} aria-hidden="true" />
                    {t('redirects.auto')}
                  </span>
                )}
                <span className={styles.status}>{redirect.status}</span>
                <div className={styles.rowActions}>
                  <Button
                    variant="icon"
                    aria-label={t('redirects.editAria', { from: redirect.from })}
                    title={t('common.edit')}
                    onClick={() => startEdit(redirect)}
                  >
                    <Pencil size={14} />
                  </Button>
                  <Button
                    variant="icon"
                    danger
                    aria-label={t('redirects.deleteAria', { from: redirect.from })}
                    title={t('common.delete')}
                    onClick={() => void handleDelete(redirect)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </div>
            ))}
            {redirects.length === 0 && (
              <div className={styles.empty}>
                {query ? t('redirects.noMatch') : t('redirects.empty')}
              </div>
            )}
          </div>
        )}

        {total > PAGE_SIZE && (
          <nav className={styles.pagination} aria-label={t('redirects.pagination')}>
            <Button
              variant="icon"
              aria-label={t('redirects.previousPage')}
              disabled={page <= 1}
              onClick={() => setPage(page - 1)}
            >
              <ChevronLeft size={16} />
            </Button>
            <span aria-live="polite">{t('redirects.range', { from: first, to: last, total })}</span>
            <Button
              variant="icon"
              aria-label={t('redirects.nextPage')}
              disabled={page >= lastPage}
              onClick={() => setPage(page + 1)}
            >
              <ChevronRight size={16} />
            </Button>
          </nav>
        )}

        <form className={styles.form} onSubmit={handleSubmit}>
          <div className={styles.formTitle}>
            {editing ? t('redirects.editTitle', { from: editing.from }) : t('redirects.new')}
          </div>
          <FieldRow>
            <Field label={t('redirects.fieldFrom')}>
              <input
                ref={fromRef}
                value={from}
                onChange={(e) => setFrom(e.target.value)}
                placeholder="/old-page"
                maxLength={512}
                spellCheck={false}
                aria-label={t('redirects.fromAria')}
              />
            </Field>
            <Field label={t('redirects.fieldTo')}>
              <input
                value={to}
                onChange={(e) => setTo(e.target.value)}
                placeholder="/articles/new-page"
                maxLength={2000}
                spellCheck={false}
                aria-label={t('redirects.toAria')}
              />
            </Field>
          </FieldRow>
          <Field label={t('redirects.fieldStatus')}>
            <select
              value={status}
              onChange={(e) => setStatus(e.target.value === '302' ? 302 : 301)}
              aria-label={t('redirects.statusAria')}
            >
              <option value={301}>{t('redirects.permanent')}</option>
              <option value={302}>{t('redirects.temporary')}</option>
            </select>
          </Field>
          <p className={styles.hint}>{t('redirects.hint')}</p>
          {formError && <div className={cx(styles.notice, styles.error)}>{formError}</div>}
          <div className={styles.formActions}>
            {editing && (
              <Button type="button" onClick={resetForm}>
                {t('common.cancel')}
              </Button>
            )}
            <Button type="submit" variant="primary" disabled={saving}>
              {editing ? t('redirects.save') : t('redirects.create')}
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
};
