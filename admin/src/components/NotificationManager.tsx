import React, { useCallback, useEffect, useId, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Bell,
  CheckCircle2,
  FileCode2,
  Inbox,
  RefreshCw,
  RotateCcw,
  Save,
  Send,
  Trash2,
  XCircle,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type {
  NotificationPreview,
  NotificationQueueItem,
  NotificationQueueStatus,
  NotificationStatus,
  NotificationTemplateDetail,
  NotificationTemplateInfo,
  NotificationTestDelivery,
} from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './NotificationManager.module.css';

interface NotificationManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

const errorMessage = (err: unknown, fallback: string) =>
  err instanceof Error ? err.message : fallback;

/**
 * Notification settings (admin): delivery status and the queue (retry / discard notifications
 * that could not be sent), test sends, and editing templates with a live preview
 */
export const NotificationManager: React.FC<NotificationManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<NotificationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadStatus = useCallback(() => {
    aegisApi
      .getNotificationStatus()
      .then(setStatus)
      .catch((err: unknown) => setError(errorMessage(err, t('notifications.loadFailed'))));
  }, [t]);

  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.title}>
          <Bell size={22} />
          <h1>{t('notifications.title')}</h1>
        </div>
        <p className={styles.description}>{t('notifications.description')}</p>
        {error && <div className={styles.error}>{error}</div>}
        {status && !status.enabled && (
          <div className={styles.notice}>
            <AlertTriangle size={18} />
            <span>{t('notifications.disabled')}</span>
          </div>
        )}
        {status?.enabled && (
          <>
            <DeliveryCard status={status} onChanged={loadStatus} onShowToast={onShowToast} />
            <TestSendCard status={status} />
            <TemplateCard status={status} onShowToast={onShowToast} />
          </>
        )}
      </div>
    </main>
  );
};

const DeliveryCard: React.FC<{
  status: NotificationStatus;
  onChanged: () => void;
  onShowToast: NotificationManagerProps['onShowToast'];
}> = ({ status, onChanged, onShowToast }) => {
  const { t, i18n } = useTranslation();
  const [filter, setFilter] = useState<NotificationQueueStatus>('failed');
  const [items, setItems] = useState<NotificationQueueItem[]>([]);
  const [total, setTotal] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    if (!status.persistent) return;
    aegisApi
      .listNotificationQueue({ status: filter, limit: 50 })
      .then((page) => {
        setItems(page.items);
        setTotal(page.total);
        setError(null);
      })
      .catch((err: unknown) => setError(errorMessage(err, t('api.notificationQueueFailed'))));
  }, [filter, status.persistent, t]);

  useEffect(() => {
    load();
  }, [load]);

  const act = async (item: NotificationQueueItem, action: 'retry' | 'discard') => {
    setBusy(item.id);
    try {
      if (action === 'retry') {
        await aegisApi.retryNotification(item.id);
        onShowToast(t('notifications.queue.retried'));
      } else {
        await aegisApi.discardNotification(item.id);
        onShowToast(t('notifications.queue.discarded'));
      }
      load();
      onChanged();
    } catch (err) {
      onShowToast(errorMessage(err, t('notifications.queue.actionFailed')), 'error');
    } finally {
      setBusy(null);
    }
  };

  const formatDate = (value: string) => new Date(value).toLocaleString(i18n.language);

  return (
    <section className={styles.card}>
      <div className={styles.cardHeader}>
        <div className={styles.cardTitle}>
          <Inbox size={18} />
          <h2>{t('notifications.delivery.heading')}</h2>
        </div>
        <Button
          variant="tool"
          onClick={() => {
            load();
            onChanged();
          }}
          title={t('notifications.refresh')}
          aria-label={t('notifications.refresh')}
        >
          <RefreshCw size={14} />
        </Button>
      </div>
      <dl className={styles.facts}>
        <div>
          <dt>{t('notifications.delivery.channels')}</dt>
          <dd>
            {status.channels.map((channel) => (
              <span key={channel} className={styles.chip}>
                {channel}
              </span>
            ))}
          </dd>
        </div>
        <div>
          <dt>{t('notifications.delivery.format')}</dt>
          <dd>
            {status.html ? t('notifications.delivery.html') : t('notifications.delivery.text')}
          </dd>
        </div>
        <div>
          <dt>{t('notifications.delivery.locale')}</dt>
          <dd>{status.locale}</dd>
        </div>
        <div>
          <dt>{t('notifications.delivery.queue')}</dt>
          <dd>
            {status.persistent
              ? t('notifications.delivery.queueDatabase', { attempts: status.max_attempts })
              : t('notifications.delivery.queueMemory')}
          </dd>
        </div>
      </dl>

      {status.persistent && (
        <>
          <div className={styles.tabs} role="tablist">
            {(['failed', 'pending'] as const).map((value) => (
              <button
                key={value}
                type="button"
                role="tab"
                aria-selected={filter === value}
                className={cx(styles.tab, filter === value && styles.tabActive)}
                onClick={() => setFilter(value)}
              >
                {t(`notifications.queue.${value}`, { count: status.queue[value] })}
              </button>
            ))}
          </div>
          {error && <div className={styles.error}>{error}</div>}
          {items.length === 0 ? (
            <p className={styles.empty}>{t(`notifications.queue.empty.${filter}`)}</p>
          ) : (
            <ul className={styles.queue}>
              {items.map((item) => (
                <li key={item.id} className={styles.queueItem}>
                  <div className={styles.queueMain}>
                    <div className={styles.queueSubject}>{item.subject || item.kind}</div>
                    <div className={styles.queueMeta}>
                      <span className={styles.chip}>{item.kind}</span>
                      <span>
                        {item.recipient === 'staff' ? t('notifications.staff') : item.recipient}
                      </span>
                      <span>· {item.channel}</span>
                      <span>· {formatDate(item.created_at)}</span>
                      <span>· {t('notifications.queue.attempts', { count: item.attempts })}</span>
                      {item.status === 'pending' && (
                        <span>
                          ·{' '}
                          {item.sending
                            ? t('notifications.queue.sending')
                            : t('notifications.queue.next', {
                                at: formatDate(item.next_attempt_at),
                              })}
                        </span>
                      )}
                    </div>
                    {item.last_error && <div className={styles.queueError}>{item.last_error}</div>}
                  </div>
                  <div className={styles.queueActions}>
                    {item.status === 'failed' && (
                      <Button
                        variant="tool"
                        onClick={() => act(item, 'retry')}
                        disabled={busy === item.id}
                      >
                        <RotateCcw size={14} />
                        <span>{t('notifications.queue.retry')}</span>
                      </Button>
                    )}
                    <Button
                      variant="icon"
                      danger
                      onClick={() => act(item, 'discard')}
                      disabled={busy === item.id || item.sending}
                      title={t('notifications.queue.discard')}
                      aria-label={t('notifications.queue.discard')}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          {total > items.length && (
            <p className={styles.hint}>
              {t('notifications.queue.more', { count: total - items.length })}
            </p>
          )}
        </>
      )}
    </section>
  );
};

const TestSendCard: React.FC<{ status: NotificationStatus }> = ({ status }) => {
  const { t } = useTranslation();
  const [kind, setKind] = useState('test');
  const [to, setTo] = useState('');
  const [staff, setStaff] = useState(false);
  const [sending, setSending] = useState(false);
  const [results, setResults] = useState<NotificationTestDelivery[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const info = status.kinds.find((k) => k.kind === kind);
  const kindId = useId();
  const toId = useId();

  const send = async (e: React.FormEvent) => {
    e.preventDefault();
    setSending(true);
    setError(null);
    setResults(null);
    try {
      setResults(
        await aegisApi.sendTestNotification({
          kind,
          to: info?.person === false ? null : to.trim() || null,
          staff: info?.staff === false ? false : staff,
        })
      );
    } catch (err) {
      setError(errorMessage(err, t('api.testNotificationFailed')));
    } finally {
      setSending(false);
    }
  };

  const canSend =
    !sending && ((info?.person !== false && to.trim() !== '') || (info?.staff !== false && staff));

  return (
    <section className={styles.card}>
      <div className={styles.cardHeader}>
        <div className={styles.cardTitle}>
          <Send size={18} />
          <h2>{t('notifications.test.heading')}</h2>
        </div>
      </div>
      <p className={styles.cardDescription}>{t('notifications.test.description')}</p>
      <form className={styles.testForm} onSubmit={send}>
        <div className={styles.fieldGroup}>
          <label htmlFor={kindId} className={styles.fieldTitle}>
            {t('notifications.kind')}
          </label>
          <select
            id={kindId}
            className={styles.select}
            value={kind}
            onChange={(e) => setKind(e.target.value)}
          >
            {status.kinds.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.kind}
                {k.disabled ? ` (${t('notifications.disabledKind')})` : ''}
              </option>
            ))}
          </select>
        </div>
        <div className={styles.fieldGroup}>
          <label htmlFor={toId} className={styles.fieldTitle}>
            {t('notifications.test.to')}
          </label>
          <Field>
            <input
              id={toId}
              type="email"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              placeholder="you@example.com"
              disabled={info?.person === false}
            />
          </Field>
        </div>
        <label className={styles.toggleLabel}>
          <input
            type="checkbox"
            className={styles.toggleInput}
            checked={staff && info?.staff !== false}
            onChange={(e) => setStaff(e.target.checked)}
            disabled={info?.staff === false}
          />
          <span>{t('notifications.test.staff')}</span>
        </label>
        <Button type="submit" variant="primary" disabled={!canSend}>
          <Send size={16} />
          <span>{sending ? t('notifications.test.sending') : t('notifications.test.send')}</span>
        </Button>
      </form>
      {status.channels.includes('email') && !status.staff_recipients && (
        <p className={styles.hint}>{t('notifications.test.noStaffRecipients')}</p>
      )}
      {error && <div className={styles.error}>{error}</div>}
      {results && (
        <ul className={styles.results}>
          {results.map((r, i) => (
            <li key={i} className={cx(styles.result, r.ok ? styles.resultOk : styles.resultFailed)}>
              {r.ok ? <CheckCircle2 size={16} /> : <XCircle size={16} />}
              <div>
                <div>
                  <strong>{r.channel}</strong> →{' '}
                  {r.recipient === 'staff' ? t('notifications.staff') : r.recipient}:{' '}
                  {r.ok ? t('notifications.test.sent', { subject: r.subject }) : r.error}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
};

/** Templates for a notification kind live under `<base>` (`audit.*` share the `audit` templates) */
const templateBase = (kind: string) => (kind.startsWith('audit.') ? 'audit' : kind);
const EXTENSIONS = ['subject', 'txt', 'html'] as const;
/** The notification a shared part is previewed with */
const PART_PREVIEW_KIND: Record<string, string> = {
  '_summary.txt': 'order.placed',
  '_comment_summary.txt': 'comment.pending',
  '_audit_title.txt': 'audit.session.create',
};

interface EditorState {
  detail: NotificationTemplateDetail;
  /** Text in the editor */
  draft: string;
}

const TemplateCard: React.FC<{
  status: NotificationStatus;
  onShowToast: NotificationManagerProps['onShowToast'];
}> = ({ status, onShowToast }) => {
  const { t } = useTranslation();
  const [templates, setTemplates] = useState<NotificationTemplateInfo[]>([]);
  // A notification kind or a shared part (`_footer.txt`, `_layout.html`…)
  const [target, setTarget] = useState('order.placed');
  const [wantStaff, setStaff] = useState(false);
  const [editors, setEditors] = useState<Record<string, EditorState>>({});
  const [preview, setPreview] = useState<NotificationPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [view, setView] = useState<'html' | 'text'>(status.html ? 'html' : 'text');
  const [saving, setSaving] = useState<string | null>(null);

  const targetId = useId();
  const isPart = target.startsWith('_');
  const kindInfo = status.kinds.find((k) => k.kind === target);
  // Kinds with templates for one audience only (account.registered: staff) are edited for it
  const staff =
    kindInfo && !kindInfo.person ? true : kindInfo && !kindInfo.staff ? false : wantStaff;
  const previewKind = isPart ? (PART_PREVIEW_KIND[target] ?? 'test') : target;
  const previewStaff = isPart ? false : staff;
  const names = useMemo(() => {
    if (isPart) return [target];
    const base = templateBase(target) + (staff ? '.staff' : '');
    return EXTENSIONS.filter((ext) => ext !== 'html' || status.html).map((ext) => `${base}.${ext}`);
  }, [isPart, target, staff, status.html]);
  const parts = templates.filter((tpl) => tpl.name.startsWith('_'));

  const refreshList = useCallback(() => {
    aegisApi
      .listNotificationTemplates()
      .then(setTemplates)
      .catch(() => setTemplates([]));
  }, []);

  useEffect(() => {
    refreshList();
  }, [refreshList]);

  useEffect(() => {
    let cancelled = false;
    Promise.all(names.map((name) => aegisApi.getNotificationTemplate(name)))
      .then((details) => {
        if (cancelled) return;
        setEditors(
          Object.fromEntries(
            details.map((detail) => [detail.name, { detail, draft: detail.current?.content ?? '' }])
          )
        );
      })
      .catch((err: unknown) => {
        if (!cancelled)
          onShowToast(errorMessage(err, t('api.notificationTemplatesFailed')), 'error');
      });
    return () => {
      cancelled = true;
    };
  }, [names, onShowToast, t]);

  const isDirty = (state: EditorState) => state.draft !== (state.detail.current?.content ?? '');
  const dirtyTemplates = useMemo(
    () =>
      Object.fromEntries(
        Object.values(editors)
          .filter((state) => isDirty(state) && state.draft !== '')
          .map((state) => [state.detail.name, state.draft])
      ),
    [editors]
  );
  const dirtyKey = JSON.stringify(dirtyTemplates);

  // Live preview with the unsaved edits
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      aegisApi
        .previewNotification({
          kind: previewKind,
          staff: previewStaff,
          templates: JSON.parse(dirtyKey) as Record<string, string>,
        })
        .then((result) => {
          if (cancelled) return;
          setPreview(result);
          setPreviewError(null);
        })
        .catch((err: unknown) => {
          if (cancelled) return;
          setPreviewError(errorMessage(err, t('api.previewNotificationFailed')));
        });
    }, 350);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [previewKind, previewStaff, dirtyKey, t]);

  const save = async (name: string) => {
    const state = editors[name];
    setSaving(name);
    try {
      const detail = await aegisApi.saveNotificationTemplate(name, state.draft);
      setEditors((prev) => ({ ...prev, [name]: { detail, draft: detail.current?.content ?? '' } }));
      refreshList();
      onShowToast(t('notifications.templates.saved', { name }));
    } catch (err) {
      onShowToast(errorMessage(err, t('api.saveNotificationTemplateFailed')), 'error');
    } finally {
      setSaving(null);
    }
  };

  const reset = async (name: string) => {
    if (!window.confirm(t('notifications.templates.resetConfirm', { name }))) return;
    setSaving(name);
    try {
      const detail = await aegisApi.resetNotificationTemplate(name);
      setEditors((prev) => ({ ...prev, [name]: { detail, draft: detail.current?.content ?? '' } }));
      refreshList();
      onShowToast(t('notifications.templates.reset', { name }));
    } catch (err) {
      onShowToast(errorMessage(err, t('api.saveNotificationTemplateFailed')), 'error');
    } finally {
      setSaving(null);
    }
  };

  return (
    <section className={styles.card}>
      <div className={styles.cardHeader}>
        <div className={styles.cardTitle}>
          <FileCode2 size={18} />
          <h2>{t('notifications.templates.heading')}</h2>
        </div>
      </div>
      <p className={styles.cardDescription}>{t('notifications.templates.description')}</p>

      <div className={styles.templateBar}>
        <div className={styles.fieldGroup}>
          <label htmlFor={targetId} className={styles.fieldTitle}>
            {t('notifications.templates.target')}
          </label>
          <select
            id={targetId}
            className={styles.select}
            value={target}
            onChange={(e) => setTarget(e.target.value)}
          >
            <optgroup label={t('notifications.templates.kinds')}>
              {status.kinds.map((k) => (
                <option key={k.kind} value={k.kind}>
                  {k.kind.startsWith('audit.') ? 'audit.*' : k.kind}
                </option>
              ))}
            </optgroup>
            <optgroup label={t('notifications.templates.parts')}>
              {parts.map((tpl) => (
                <option key={tpl.name} value={tpl.name}>
                  {tpl.name}
                </option>
              ))}
            </optgroup>
          </select>
        </div>
        {!isPart && (
          <div
            className={styles.segmented}
            role="group"
            aria-label={t('notifications.templates.audience')}
          >
            <button
              type="button"
              className={cx(styles.segment, !staff && styles.segmentActive)}
              aria-pressed={!staff}
              onClick={() => setStaff(false)}
              disabled={kindInfo?.person === false}
            >
              {t('notifications.templates.person')}
            </button>
            <button
              type="button"
              className={cx(styles.segment, staff && styles.segmentActive)}
              aria-pressed={staff}
              onClick={() => setStaff(true)}
              disabled={kindInfo?.staff === false}
            >
              {t('notifications.staff')}
            </button>
          </div>
        )}
      </div>

      <div className={styles.templateGrid}>
        <div className={styles.editors}>
          {names.map((name) => {
            const state = editors[name];
            if (!state) return null;
            const dirty = isDirty(state);
            const source = state.detail.current?.source;
            return (
              <div key={name} className={styles.editor} role="group" aria-label={name}>
                <div className={styles.editorHeader}>
                  <code className={styles.editorName}>{name}</code>
                  <span className={cx(styles.badge, source && styles[`badge_${source}`])}>
                    {source
                      ? t(`notifications.templates.source.${source}`)
                      : t('notifications.templates.source.none')}
                  </span>
                  {dirty && (
                    <span className={styles.dirty}>{t('notifications.templates.unsaved')}</span>
                  )}
                  <div className={styles.editorActions}>
                    {source === 'custom' && (
                      <Button
                        variant="tool"
                        onClick={() => reset(name)}
                        disabled={saving === name}
                        title={t('notifications.templates.resetHint')}
                      >
                        <RotateCcw size={14} />
                        <span>{t('notifications.templates.resetButton')}</span>
                      </Button>
                    )}
                    <Button
                      variant="tool"
                      onClick={() => save(name)}
                      disabled={!dirty || state.draft === '' || saving === name}
                    >
                      <Save size={14} />
                      <span>{t('notifications.templates.save')}</span>
                    </Button>
                  </div>
                </div>
                <textarea
                  className={cx(styles.textarea, name.endsWith('.subject') && styles.subjectArea)}
                  value={state.draft}
                  onChange={(e) =>
                    setEditors((prev) => ({
                      ...prev,
                      [name]: { ...prev[name], draft: e.target.value },
                    }))
                  }
                  placeholder={
                    name.endsWith('.html')
                      ? t('notifications.templates.htmlPlaceholder')
                      : t('notifications.templates.fallbackPlaceholder')
                  }
                  spellCheck={false}
                  aria-label={name}
                />
              </div>
            );
          })}
          <p className={styles.hint}>{t('notifications.templates.variablesHint')}</p>
        </div>

        <div className={styles.preview}>
          <div className={styles.previewHeader}>
            <span className={styles.fieldTitle}>
              {t('notifications.templates.preview', { kind: previewKind })}
            </span>
            {status.html && (
              <div className={styles.segmented} role="group">
                {(['html', 'text'] as const).map((value) => (
                  <button
                    key={value}
                    type="button"
                    className={cx(styles.segment, view === value && styles.segmentActive)}
                    aria-pressed={view === value}
                    onClick={() => setView(value)}
                  >
                    {t(`notifications.templates.view.${value}`)}
                  </button>
                ))}
              </div>
            )}
          </div>
          {previewError && <div className={styles.error}>{previewError}</div>}
          {preview && (
            <>
              <div className={styles.previewSubject}>{preview.subject}</div>
              {view === 'html' && preview.html ? (
                <iframe
                  className={styles.previewFrame}
                  title={t('notifications.templates.previewFrame')}
                  sandbox=""
                  srcDoc={preview.html}
                />
              ) : (
                <pre className={styles.previewText}>{preview.body}</pre>
              )}
            </>
          )}
        </div>
      </div>
    </section>
  );
};
