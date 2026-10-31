import React, { useEffect, useRef, useState } from 'react';
import {
  Archive,
  Check,
  ChevronLeft,
  ChevronRight,
  Inbox,
  Mail,
  MailOpen,
  Paperclip,
  RefreshCw,
  Search,
  ShieldAlert,
  Trash2,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type {
  ContactFieldValue,
  ContactForm,
  ContactMessage,
  ContactMessagePage,
  ContactStatus,
} from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { formatSize } from '../utils/media';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import shop from './Shop.module.css';
import styles from './ContactInbox.module.css';

const PAGE_SIZE = 25;

const FILTERS: (ContactStatus | '')[] = ['new', 'read', 'done', 'spam', ''];

const STATUS_BADGE: Record<ContactStatus, string | undefined> = {
  new: shop.warn,
  read: undefined,
  done: shop.good,
  spam: shop.bad,
};

type Action = ContactStatus | 'delete';

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

/** The message body for the list: the `message` field, else the longest other answer */
const preview = (message: ContactMessage): string => {
  const others = message.fields.filter(
    (f) => f.value && !['name', 'email', 'subject'].includes(f.name) && f.value !== message.subject
  );
  const body =
    others.find((f) => f.name === 'message') ??
    others.reduce<(typeof others)[number] | undefined>(
      (longest, f) => (!longest || f.value.length > longest.value.length ? f : longest),
      undefined
    );
  return body ? body.value.replace(/\s+/g, ' ').slice(0, 140) : '';
};

/** `mailto:` link that answers the message from the user's mail app */
const replyLink = (message: ContactMessage): string => {
  const subject = message.subject ? `Re: ${message.subject}` : `Re: ${message.form_title}`;
  return `mailto:${encodeURIComponent(message.email)}?subject=${encodeURIComponent(subject)}`;
};

interface ContactInboxProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Messages from the contact forms: read, mark as done or spam, delete, answer by email */
export const ContactInbox: React.FC<ContactInboxProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<ContactStatus | ''>('new');
  const [form, setForm] = useState<string>('');
  const [forms, setForms] = useState<ContactForm[]>([]);
  // `draft` is what is typed; `query` is what the list is filtered by (applied on submit)
  const [draft, setDraft] = useState<string>('');
  const [query, setQuery] = useState<string>('');
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<ContactMessagePage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<boolean>(false);
  const [open, setOpen] = useState<ContactMessage | null>(null);
  // The parent's toast callback may change on every render; loading must not follow it
  const toastRef = useRef(onShowToast);
  useEffect(() => {
    toastRef.current = onShowToast;
  });

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listContactForms()
      .then((loaded) => {
        if (!cancelled) setForms(loaded);
      })
      .catch(() => {
        // The list below reports a disabled inbox
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listContactMessages({ status, form, q: query, limit: PAGE_SIZE, offset })
      .then((loaded) => {
        if (cancelled) return;
        setPage(loaded);
        setSelected(new Set());
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.status === 404
            ? i18n.t('contact.disabled')
            : messageOf(err, i18n.t('api.contactMessagesFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, form, query, offset, reloadCount]);

  const reload = () => setReloadCount((n) => n + 1);

  const search = (value: string) => {
    setDraft(value);
    setQuery(value.trim());
    setOffset(0);
  };

  const download = async (message: ContactMessage, field: ContactFieldValue) => {
    try {
      await aegisApi.downloadContactAttachment(message.id, field.name, field.value);
    } catch (err) {
      toastRef.current(messageOf(err, t('api.contactAttachmentFailed')), 'error');
    }
  };

  const apply = async (ids: string[], action: Action) => {
    if (ids.length === 0) return;
    if (action === 'delete' && !window.confirm(t('contact.confirmDelete', { count: ids.length }))) {
      return;
    }
    setBusy(true);
    try {
      if (ids.length === 1) {
        if (action === 'delete') await aegisApi.deleteContactMessage(ids[0]);
        else await aegisApi.updateContactMessage(ids[0], action);
      } else {
        await aegisApi.bulkContactMessages(ids, action);
      }
      toastRef.current(t(`contact.done.${action}`, { count: ids.length }));
      setOpen(null);
      reload();
    } catch (err) {
      toastRef.current(messageOf(err, t('api.updateContactMessageFailed')), 'error');
    } finally {
      setBusy(false);
    }
  };

  /** Opens a message; a new one becomes read */
  const openMessage = (message: ContactMessage) => {
    setOpen(message);
    if (message.status !== 'new') return;
    aegisApi
      .updateContactMessage(message.id, 'read')
      .then(() => {
        setOpen((current) =>
          current?.id === message.id ? { ...current, status: 'read' } : current
        );
        reload();
      })
      .catch(() => {
        // Stays new; the editor can mark it by hand
      });
  };

  const messages = page?.messages ?? [];
  const allSelected = messages.length > 0 && messages.every((m) => selected.has(m.id));
  const toggle = (id: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const selectedIds = [...selected];

  return (
    <main className={shop.main}>
      <div className={shop.column}>
        <div className={shop.heading}>
          <div className={shop.title}>
            <Inbox size={22} />
            <h1>{t('contact.title')}</h1>
            {page && <span className={shop.count}>{page.total}</span>}
          </div>
          <div className={shop.headingActions}>
            {forms.length > 1 && (
              <select
                className={styles.formSelect}
                aria-label={t('contact.formFilter')}
                value={form}
                onChange={(e) => {
                  setForm(e.target.value);
                  setOffset(0);
                }}
              >
                <option value="">{t('contact.allForms')}</option>
                {forms.map((f) => (
                  <option key={f.name} value={f.name}>
                    {f.title}
                  </option>
                ))}
              </select>
            )}
            <Button variant="tool" onClick={reload}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        <form
          className={styles.search}
          role="search"
          onSubmit={(e) => {
            e.preventDefault();
            search(draft);
          }}
        >
          <Search size={14} aria-hidden="true" />
          <input
            type="search"
            className={styles.searchInput}
            aria-label={t('contact.search')}
            placeholder={t('contact.searchPlaceholder')}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          {query && (
            <Button variant="tool" type="button" onClick={() => search('')}>
              <X size={14} />
              <span>{t('contact.clearSearch')}</span>
            </Button>
          )}
        </form>

        {forms.length > 0 && (
          <p className={styles.formLinks}>
            {t('contact.publicForms')}{' '}
            {forms.map((f, i) => (
              <React.Fragment key={f.name}>
                {i > 0 && ' · '}
                <a href={f.path} target="_blank" rel="noreferrer">
                  {f.title}
                </a>
              </React.Fragment>
            ))}
          </p>
        )}

        <div className={shop.filters} role="tablist" aria-label={t('contact.statusFilter')}>
          {FILTERS.map((value) => (
            <button
              key={value || 'all'}
              type="button"
              role="tab"
              aria-selected={status === value}
              className={cx(shop.filter, status === value && shop.active)}
              onClick={() => {
                setStatus(value);
                setOffset(0);
              }}
            >
              {value ? t(`contact.status.${value}`) : t('contact.all')}
              {value && page && (page.counts[value] ?? 0) > 0 && (
                <span className={styles.filterCount}>{page.counts[value]}</span>
              )}
            </button>
          ))}
        </div>

        {error && <div className={cx(shop.notice, shop.error)}>{error}</div>}

        {page && selectedIds.length > 0 && (
          <div className={styles.bulkBar} role="toolbar" aria-label={t('contact.bulkActions')}>
            <span>{t('contact.selected', { count: selectedIds.length })}</span>
            <StatusActions
              status={status || undefined}
              disabled={busy}
              onAction={(action) => apply(selectedIds, action)}
            />
          </div>
        )}

        {page && (
          <div className={shop.tableWrap}>
            <table className={shop.table}>
              <thead>
                <tr>
                  <th className={styles.checkCell}>
                    <input
                      type="checkbox"
                      checked={allSelected}
                      onChange={() =>
                        setSelected(allSelected ? new Set() : new Set(messages.map((m) => m.id)))
                      }
                      aria-label={t('contact.selectAll')}
                    />
                  </th>
                  <th>{t('contact.columns.from')}</th>
                  <th>{t('contact.columns.message')}</th>
                  <th>{t('contact.columns.form')}</th>
                  <th>{t('contact.columns.status')}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {messages.map((message) => (
                  <tr key={message.id} className={cx(message.status === 'new' && styles.unread)}>
                    <td className={styles.checkCell}>
                      <input
                        type="checkbox"
                        checked={selected.has(message.id)}
                        onChange={() => toggle(message.id)}
                        aria-label={t('contact.selectOne', {
                          name: message.name || message.email || message.id,
                        })}
                      />
                    </td>
                    <td className={styles.fromCell}>
                      <div className={shop.productNames}>
                        <span>{message.name || t('contact.noName')}</span>
                        {message.email && (
                          <span className={cx(shop.muted, styles.email)}>{message.email}</span>
                        )}
                      </div>
                    </td>
                    <td className={styles.messageCell}>
                      <button
                        type="button"
                        className={styles.openButton}
                        onClick={() => openMessage(message)}
                      >
                        <span className={styles.subject}>
                          {message.subject || t('contact.noSubject')}
                          {message.fields.some((f) => f.file) && (
                            <Paperclip
                              size={12}
                              className={styles.clip}
                              aria-label={t('contact.hasAttachment')}
                            />
                          )}
                        </span>
                        <span className={styles.preview}>{preview(message)}</span>
                      </button>
                      <div className={shop.muted}>{formatDateTime(message.created_at)}</div>
                    </td>
                    <td>{message.form_title}</td>
                    <td>
                      <span className={cx(shop.badge, STATUS_BADGE[message.status])}>
                        {t(`contact.status.${message.status}`)}
                      </span>
                    </td>
                    <td>
                      <div className={shop.rowActions}>
                        <StatusActions
                          status={message.status}
                          disabled={busy}
                          compact
                          onAction={(action) => apply([message.id], action)}
                        />
                      </div>
                    </td>
                  </tr>
                ))}
                {messages.length === 0 && (
                  <tr>
                    <td colSpan={6} className={shop.empty}>
                      {query
                        ? t('contact.emptySearch', { query })
                        : status
                          ? t('contact.emptyStatus', { status: t(`contact.status.${status}`) })
                          : t('contact.empty')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}

        {page && page.total > PAGE_SIZE && (
          <div className={shop.pagination}>
            <Button
              variant="tool"
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
            >
              <ChevronLeft size={14} />
              <span>{t('contact.newer')}</span>
            </Button>
            <span>
              {t('contact.range', {
                from: offset + 1,
                to: Math.min(offset + PAGE_SIZE, page.total),
                total: page.total,
              })}
            </span>
            <Button
              variant="tool"
              disabled={offset + PAGE_SIZE >= page.total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
            >
              <span>{t('contact.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {open && (
        <Modal
          title={open.subject || t('contact.noSubject')}
          icon={<Mail size={18} />}
          onClose={() => setOpen(null)}
          wide
        >
          <div className={styles.detail}>
            <dl className={styles.meta}>
              <dt>{t('contact.columns.from')}</dt>
              <dd>
                {open.name || t('contact.noName')}
                {open.email && <span className={shop.muted}> &lt;{open.email}&gt;</span>}
              </dd>
              <dt>{t('contact.received')}</dt>
              <dd>{formatDateTime(open.created_at)}</dd>
              <dt>{t('contact.columns.form')}</dt>
              <dd>{open.form_title}</dd>
              <dt>{t('contact.columns.status')}</dt>
              <dd>
                <span className={cx(shop.badge, STATUS_BADGE[open.status])}>
                  {t(`contact.status.${open.status}`)}
                </span>
              </dd>
            </dl>
            <dl className={styles.fields}>
              {open.fields.map((field) => (
                <React.Fragment key={field.name}>
                  <dt>{field.label}</dt>
                  <dd className={styles.value}>
                    {field.file ? (
                      <button
                        type="button"
                        className={styles.attachment}
                        onClick={() => download(open, field)}
                      >
                        <Paperclip size={14} />
                        <span>{field.file.filename}</span>
                        <span className={shop.muted}>({formatSize(field.file.size)})</span>
                      </button>
                    ) : field.value === 'yes' && field.name !== 'message' ? (
                      t('contact.checked')
                    ) : (
                      field.value || <span className={shop.muted}>—</span>
                    )}
                  </dd>
                </React.Fragment>
              ))}
            </dl>
            {(open.ip || open.user_agent) && (
              <p className={cx(shop.muted, styles.client)}>
                {open.ip && <span className={shop.mono}>{open.ip}</span>}
                {open.ip && open.user_agent && ' · '}
                {open.user_agent}
              </p>
            )}
            <div className={styles.detailActions}>
              {open.email && (
                <a className={styles.replyLink} href={replyLink(open)}>
                  <Mail size={14} />
                  <span>{t('contact.reply')}</span>
                </a>
              )}
              <StatusActions
                status={open.status}
                disabled={busy}
                onAction={(action) => apply([open.id], action)}
              />
            </div>
          </div>
        </Modal>
      )}
    </main>
  );
};

interface StatusActionsProps {
  /** Current status (of the row, or of the list for bulk actions; `undefined` for all) */
  status?: ContactStatus;
  disabled: boolean;
  /** Icon buttons (rows) instead of labelled buttons */
  compact?: boolean;
  onAction: (action: Action) => void;
}

const StatusActions: React.FC<StatusActionsProps> = ({ status, disabled, compact, onAction }) => {
  const { t } = useTranslation();
  const actions: Array<{ action: Action; icon: React.ReactNode }> = [];
  if (status !== 'done') actions.push({ action: 'done', icon: <Check size={14} /> });
  if (status === 'new' || status === undefined) {
    actions.push({ action: 'read', icon: <MailOpen size={14} /> });
  }
  if (status === 'read' || status === 'done' || status === 'spam') {
    actions.push({ action: 'new', icon: <Archive size={14} /> });
  }
  if (status !== 'spam') actions.push({ action: 'spam', icon: <ShieldAlert size={14} /> });
  actions.push({ action: 'delete', icon: <Trash2 size={14} /> });
  return (
    <>
      {actions.map(({ action, icon }) => {
        const label = t(`contact.actions.${action}`);
        return (
          <Button
            key={action}
            variant={compact ? 'icon' : 'tool'}
            danger={action === 'delete'}
            disabled={disabled}
            onClick={() => onAction(action)}
            title={compact ? label : undefined}
            aria-label={compact ? label : undefined}
          >
            {icon}
            {!compact && <span>{label}</span>}
          </Button>
        );
      })}
    </>
  );
};
