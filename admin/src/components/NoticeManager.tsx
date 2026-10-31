import React, { useEffect, useRef, useState } from 'react';
import { Megaphone, Pencil, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type { Notice, NoticeStyle } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime, fromDateTimeLocal, toDateTimeLocal } from '../utils/format';
import { Button } from './ui/Button';
import { Field, FieldRow } from './ui/Field';
import styles from './NoticeManager.module.css';

interface NoticeManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const STYLES: NoticeStyle[] = ['info', 'success', 'warning', 'alert'];
/** The server shows at most this many notices at once */
const MAX_SHOWN = 3;

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface Draft {
  message: string;
  linkUrl: string;
  linkLabel: string;
  style: NoticeStyle;
  startAt: string;
  endAt: string;
  dismissible: boolean;
  enabled: boolean;
}

const EMPTY: Draft = {
  message: '',
  linkUrl: '',
  linkLabel: '',
  style: 'info',
  startAt: '',
  endAt: '',
  dismissible: true,
  enabled: true,
};

const draftOf = (notice: Notice): Draft => ({
  message: notice.message,
  linkUrl: notice.link_url,
  linkLabel: notice.link_label,
  style: notice.style,
  startAt: notice.start_at ? toDateTimeLocal(notice.start_at) : '',
  endAt: notice.end_at ? toDateTimeLocal(notice.end_at) : '',
  dismissible: notice.dismissible,
  enabled: notice.enabled,
});

/** Banners for every page of the site (maintenance, sales): message, link, kind, period, close button */
export const NoticeManager: React.FC<NoticeManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [notices, setNotices] = useState<Notice[] | null>(null);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Notice | null>(null);
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [saving, setSaving] = useState<boolean>(false);
  const [formError, setFormError] = useState<string | null>(null);
  const messageRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listNotices()
      .then((result) => {
        if (cancelled) return;
        setNotices(result.notices);
        setError(null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(messageOf(err, t('notices.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount, t]);

  const change = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((prev) => ({ ...prev, [key]: value }));

  const resetForm = () => {
    setEditing(null);
    setDraft(EMPTY);
    setFormError(null);
  };

  const startEdit = (notice: Notice) => {
    setEditing(notice);
    setDraft(draftOf(notice));
    setFormError(null);
    messageRef.current?.focus();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!draft.message.trim()) {
      setFormError(t('notices.messageRequired'));
      return;
    }
    const start = fromDateTimeLocal(draft.startAt);
    const end = fromDateTimeLocal(draft.endAt);
    if (start && end && end <= start) {
      setFormError(t('notices.endBeforeStart'));
      return;
    }
    setSaving(true);
    setFormError(null);
    try {
      const payload = {
        message: draft.message.trim(),
        link_url: draft.linkUrl.trim(),
        link_label: draft.linkLabel.trim(),
        style: draft.style,
        start_at: start ?? null,
        end_at: end ?? null,
        dismissible: draft.dismissible,
        enabled: draft.enabled,
      };
      if (editing) {
        await aegisApi.updateNotice(editing.id, payload);
        onShowToast(t('notices.updated'));
      } else {
        await aegisApi.createNotice(payload);
        onShowToast(t('notices.created'));
      }
      resetForm();
      setReloadCount((n) => n + 1);
    } catch (err) {
      // The server's reasons are English: show the field it refused, in the UI language
      const field =
        err instanceof ApiError && err.code === 'invalid_notice' ? err.field : undefined;
      setFormError(
        err instanceof ApiError && err.code === 'too_many_notices'
          ? t('notices.tooMany')
          : field
            ? `${t(`notices.field.${field}` as 'notices.field.message')}: ${err instanceof Error ? err.message : ''}`
            : messageOf(err, t('notices.saveFailed'))
      );
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (notice: Notice) => {
    if (!window.confirm(t('notices.confirmDelete'))) return;
    try {
      await aegisApi.deleteNotice(notice.id);
      if (editing?.id === notice.id) resetForm();
      setReloadCount((n) => n + 1);
      onShowToast(t('notices.deleted'));
    } catch (err) {
      onShowToast(messageOf(err, t('notices.deleteFailed')), 'error');
    }
  };

  const statusLabel = (status: Notice['status']): string =>
    status === 'active'
      ? t('notices.statusActive')
      : status === 'scheduled'
        ? t('notices.statusScheduled')
        : status === 'expired'
          ? t('notices.statusExpired')
          : t('notices.statusDisabled');

  const period = (notice: Notice): string => {
    if (!notice.start_at && !notice.end_at) return t('notices.always');
    return t('notices.period', {
      from: notice.start_at ? formatDateTime(notice.start_at) : t('notices.now'),
      to: notice.end_at ? formatDateTime(notice.end_at) : t('notices.noEnd'),
    });
  };

  const activeCount = notices?.filter((n) => n.status === 'active').length ?? 0;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Megaphone size={22} />
          <h1>{t('notices.title')}</h1>
          {notices && <span className={styles.count}>{notices.length}</span>}
        </div>
        <p className={styles.description}>{t('notices.description', { count: MAX_SHOWN })}</p>
        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}
        {activeCount > MAX_SHOWN && (
          <div className={styles.notice}>{t('notices.tooManyActive', { count: MAX_SHOWN })}</div>
        )}

        {notices && (
          <div className={styles.list}>
            {notices.map((notice) => (
              <div key={notice.id} className={styles.row}>
                <span
                  className={cx(styles.swatch, styles[`swatch_${notice.style}`])}
                  title={t(`notices.style.${notice.style}` as 'notices.style.info')}
                />
                <div className={styles.rowMain}>
                  <div className={styles.rowMessage}>{notice.message}</div>
                  <div className={styles.rowMeta}>
                    {period(notice)}
                    {notice.link_url && ` · ${notice.link_url}`}
                    {!notice.dismissible && ` · ${t('notices.notDismissible')}`}
                  </div>
                </div>
                <span className={cx(styles.badge, styles[`badge_${notice.status}`])}>
                  {statusLabel(notice.status)}
                </span>
                <div className={styles.rowActions}>
                  <Button
                    variant="icon"
                    aria-label={t('notices.editAria', { message: notice.message })}
                    title={t('common.edit')}
                    onClick={() => startEdit(notice)}
                  >
                    <Pencil size={14} />
                  </Button>
                  <Button
                    variant="icon"
                    danger
                    aria-label={t('notices.deleteAria', { message: notice.message })}
                    title={t('common.delete')}
                    onClick={() => void handleDelete(notice)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </div>
            ))}
            {notices.length === 0 && <div className={styles.empty}>{t('notices.empty')}</div>}
          </div>
        )}

        <form className={styles.form} onSubmit={handleSubmit}>
          <div className={styles.formTitle}>
            {editing ? t('notices.editTitle') : t('notices.new')}
          </div>
          <Field label={t('notices.field.message')}>
            <textarea
              ref={messageRef}
              value={draft.message}
              onChange={(e) => change('message', e.target.value)}
              maxLength={500}
              rows={2}
              aria-label={t('notices.messageAria')}
            />
          </Field>
          <FieldRow>
            <Field label={t('notices.field.link_url')}>
              <input
                value={draft.linkUrl}
                onChange={(e) => change('linkUrl', e.target.value)}
                placeholder="/status"
                maxLength={2000}
                spellCheck={false}
                aria-label={t('notices.linkUrlAria')}
              />
            </Field>
            <Field label={t('notices.field.link_label')}>
              <input
                value={draft.linkLabel}
                onChange={(e) => change('linkLabel', e.target.value)}
                placeholder={t('notices.linkLabelPlaceholder')}
                maxLength={60}
                aria-label={t('notices.linkLabelAria')}
              />
            </Field>
          </FieldRow>
          <Field label={t('notices.field.style')}>
            <select
              value={draft.style}
              onChange={(e) => change('style', e.target.value as NoticeStyle)}
              aria-label={t('notices.styleAria')}
            >
              {STYLES.map((value) => (
                <option key={value} value={value}>
                  {t(`notices.style.${value}` as 'notices.style.info')}
                </option>
              ))}
            </select>
          </Field>
          <FieldRow>
            <Field label={t('notices.field.start_at')}>
              <input
                type="datetime-local"
                value={draft.startAt}
                onChange={(e) => change('startAt', e.target.value)}
                aria-label={t('notices.startAria')}
              />
            </Field>
            <Field label={t('notices.field.end_at')}>
              <input
                type="datetime-local"
                value={draft.endAt}
                onChange={(e) => change('endAt', e.target.value)}
                aria-label={t('notices.endAria')}
              />
            </Field>
          </FieldRow>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={draft.dismissible}
              onChange={(e) => change('dismissible', e.target.checked)}
            />
            {t('notices.dismissible')}
          </label>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(e) => change('enabled', e.target.checked)}
            />
            {t('notices.enabled')}
          </label>
          <p className={styles.hint}>{t('notices.hint')}</p>
          {formError && <div className={cx(styles.notice, styles.error)}>{formError}</div>}
          <div className={styles.formActions}>
            {editing && (
              <Button type="button" onClick={resetForm}>
                {t('common.cancel')}
              </Button>
            )}
            <Button type="submit" variant="primary" disabled={saving}>
              {editing ? t('notices.save') : t('notices.create')}
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
};
