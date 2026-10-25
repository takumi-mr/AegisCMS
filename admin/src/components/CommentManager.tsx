import React, { useEffect, useState } from 'react';
import {
  Check,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  MessageSquare,
  RefreshCw,
  Reply,
  ShieldAlert,
  Trash2,
  Undo2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type {
  AdminComment,
  CommentBulkAction,
  CommentPage,
  CommentSettings,
  CommentStatus,
} from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import shop from './Shop.module.css';
import styles from './CommentManager.module.css';

const PAGE_SIZE = 25;

const FILTERS: (CommentStatus | '')[] = ['pending', 'approved', 'spam', 'trash', ''];

const STATUS_BADGE: Record<CommentStatus, string | undefined> = {
  pending: shop.warn,
  approved: shop.good,
  spam: shop.bad,
  trash: undefined,
};

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface CommentManagerProps {
  /** Admins may change the approval settings; editors see them */
  isAdmin: boolean;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Readers' comments for editors: approve, hold, mark as spam, trash, delete and reply */
export const CommentManager: React.FC<CommentManagerProps> = ({ isAdmin, onShowToast }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<CommentStatus | ''>('pending');
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<CommentPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<boolean>(false);
  const [replyTo, setReplyTo] = useState<AdminComment | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listComments({ status, limit: PAGE_SIZE, offset })
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
            ? i18n.t('comments.disabled')
            : messageOf(err, i18n.t('comments.loadFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, offset, reloadCount]);

  const reload = () => setReloadCount((n) => n + 1);

  const apply = async (ids: string[], action: CommentBulkAction) => {
    if (ids.length === 0) return;
    if (
      action === 'delete' &&
      !window.confirm(t('comments.confirmDelete', { count: ids.length }))
    ) {
      return;
    }
    setBusy(true);
    try {
      if (ids.length === 1) {
        if (action === 'delete') await aegisApi.deleteComment(ids[0]);
        else await aegisApi.moderateComment(ids[0], action);
      } else {
        await aegisApi.bulkComments(ids, action);
      }
      onShowToast(t(`comments.done.${action}`, { count: ids.length }));
      reload();
    } catch (err) {
      onShowToast(messageOf(err, t('api.moderateCommentFailed')), 'error');
    } finally {
      setBusy(false);
    }
  };

  const comments = page?.comments ?? [];
  const allSelected = comments.length > 0 && comments.every((c) => selected.has(c.id));
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
            <MessageSquare size={22} />
            <h1>{t('comments.title')}</h1>
            {page && <span className={shop.count}>{page.total}</span>}
          </div>
          <div className={shop.headingActions}>
            <Button variant="tool" onClick={reload}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        <ModerationSettingsPanel isAdmin={isAdmin} onShowToast={onShowToast} />

        <div className={shop.filters} role="tablist" aria-label={t('comments.statusFilter')}>
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
              {value ? t(`comments.status.${value}`) : t('comments.all')}
              {value && page && page.counts[value] > 0 && (
                <span className={styles.filterCount}>{page.counts[value]}</span>
              )}
            </button>
          ))}
        </div>

        {error && <div className={cx(shop.notice, shop.error)}>{error}</div>}

        {page && selectedIds.length > 0 && (
          <div className={styles.bulkBar} role="toolbar" aria-label={t('comments.bulkActions')}>
            <span>{t('comments.selected', { count: selectedIds.length })}</span>
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
                        setSelected(allSelected ? new Set() : new Set(comments.map((c) => c.id)))
                      }
                      aria-label={t('comments.selectAll')}
                    />
                  </th>
                  <th>{t('comments.columns.author')}</th>
                  <th>{t('comments.columns.comment')}</th>
                  <th>{t('comments.columns.content')}</th>
                  <th>{t('comments.columns.status')}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {comments.map((comment) => (
                  <tr key={comment.id}>
                    <td className={styles.checkCell}>
                      <input
                        type="checkbox"
                        checked={selected.has(comment.id)}
                        onChange={() => toggle(comment.id)}
                        aria-label={t('comments.selectOne', { name: comment.author_name })}
                      />
                    </td>
                    <td>
                      <div className={shop.productNames}>
                        <span>
                          {comment.author_name}
                          {comment.author_user_id && (
                            <span className={cx(shop.badge, styles.staffBadge)}>
                              {t('comments.staff')}
                            </span>
                          )}
                        </span>
                        {comment.author_email && (
                          <a className={styles.email} href={`mailto:${comment.author_email}`}>
                            {comment.author_email}
                          </a>
                        )}
                        {comment.ip && (
                          <span className={cx(shop.muted, shop.mono)}>{comment.ip}</span>
                        )}
                      </div>
                    </td>
                    <td className={styles.commentCell}>
                      <div className={shop.muted}>
                        {formatDateTime(comment.created_at)}
                        {comment.parent_author &&
                          ` · ${t('comments.inReplyTo', { name: comment.parent_author })}`}
                      </div>
                      <p className={styles.body}>{comment.body}</p>
                    </td>
                    <td>
                      {comment.content ? (
                        comment.content.url ? (
                          <a
                            className={styles.contentLink}
                            href={`${comment.content.url}#comment-${comment.id}`}
                            target="_blank"
                            rel="noreferrer"
                          >
                            {comment.content.title}
                            <ExternalLink size={12} />
                          </a>
                        ) : (
                          comment.content.title
                        )
                      ) : (
                        <span className={shop.muted}>{t('comments.contentGone')}</span>
                      )}
                    </td>
                    <td>
                      <span className={cx(shop.badge, STATUS_BADGE[comment.status])}>
                        {t(`comments.status.${comment.status}`)}
                      </span>
                    </td>
                    <td>
                      <div className={shop.rowActions}>
                        {(comment.status === 'approved' || comment.status === 'pending') && (
                          <Button
                            variant="icon"
                            disabled={busy}
                            onClick={() => setReplyTo(comment)}
                            title={t('comments.actions.reply')}
                            aria-label={t('comments.actions.reply')}
                          >
                            <Reply size={14} />
                          </Button>
                        )}
                        <StatusActions
                          status={comment.status}
                          disabled={busy}
                          compact
                          onAction={(action) => apply([comment.id], action)}
                        />
                      </div>
                    </td>
                  </tr>
                ))}
                {comments.length === 0 && (
                  <tr>
                    <td colSpan={6} className={shop.empty}>
                      {status
                        ? t('comments.emptyStatus', { status: t(`comments.status.${status}`) })
                        : t('comments.empty')}
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
              <span>{t('comments.newer')}</span>
            </Button>
            <span>
              {t('comments.range', {
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
              <span>{t('comments.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {replyTo && (
        <ReplyModal
          comment={replyTo}
          onClose={() => setReplyTo(null)}
          onReplied={() => {
            setReplyTo(null);
            onShowToast(t('comments.replied'));
            reload();
          }}
          onError={(message) => onShowToast(message, 'error')}
        />
      )}
    </main>
  );
};

interface StatusActionsProps {
  /** Current status (of the row, or of the list for bulk actions; `undefined` for all) */
  status?: CommentStatus;
  disabled: boolean;
  /** Icon buttons (rows) instead of labelled buttons (bulk bar) */
  compact?: boolean;
  onAction: (action: CommentBulkAction) => void;
}

/** The moves that make sense from a status: approve / hold, spam, trash / restore, delete */
const StatusActions: React.FC<StatusActionsProps> = ({ status, disabled, compact, onAction }) => {
  const { t } = useTranslation();
  const actions: { action: CommentBulkAction; icon: React.ReactNode; danger?: boolean }[] = [];
  if (status !== 'approved') actions.push({ action: 'approved', icon: <Check size={14} /> });
  if (status === 'approved') actions.push({ action: 'pending', icon: <Undo2 size={14} /> });
  if (status !== 'spam') actions.push({ action: 'spam', icon: <ShieldAlert size={14} /> });
  if (status !== 'trash') actions.push({ action: 'trash', icon: <Trash2 size={14} /> });
  if (status === 'spam' || status === 'trash' || status === undefined) {
    actions.push({ action: 'delete', icon: <Trash2 size={14} />, danger: true });
  }
  return (
    <>
      {actions.map(({ action, icon, danger }) => {
        const label = t(`comments.actions.${action}`);
        return compact ? (
          <Button
            key={action}
            variant="icon"
            danger={danger}
            disabled={disabled}
            onClick={() => onAction(action)}
            title={label}
            aria-label={label}
          >
            {icon}
          </Button>
        ) : (
          <Button
            key={action}
            variant="tool"
            danger={danger}
            disabled={disabled}
            onClick={() => onAction(action)}
          >
            {icon}
            <span>{label}</span>
          </Button>
        );
      })}
    </>
  );
};

interface ModerationSettingsPanelProps {
  isAdmin: boolean;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Approval on / off and auto-approval of known authors (saved in the database, admin only) */
const ModerationSettingsPanel: React.FC<ModerationSettingsPanelProps> = ({
  isAdmin,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const [settings, setSettings] = useState<CommentSettings | null>(null);
  const [saving, setSaving] = useState<boolean>(false);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getCommentSettings()
      .then((loaded) => {
        if (!cancelled) setSettings(loaded);
      })
      .catch((err) => console.warn('Could not load the comment settings:', err));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!settings) return null;

  const save = async (overrides: CommentSettings['overrides']) => {
    // Show the new values right away; the server's answer replaces them (or the old ones come back)
    const previous = settings;
    setSettings({
      ...settings,
      overrides,
      policy: {
        require_approval: overrides.require_approval ?? settings.defaults.require_approval,
        auto_approve_known: overrides.auto_approve_known ?? settings.defaults.auto_approve_known,
      },
    });
    setSaving(true);
    try {
      setSettings(await aegisApi.updateCommentSettings(overrides));
      onShowToast(t('comments.settings.saved'));
    } catch (err) {
      setSettings(previous);
      onShowToast(messageOf(err, t('api.updateCommentSettingsFailed')), 'error');
    } finally {
      setSaving(false);
    }
  };

  const { policy, overrides, defaults } = settings;
  const changed = overrides.require_approval !== null || overrides.auto_approve_known !== null;
  const disabled = !isAdmin || saving;
  const onOff = (value: boolean) =>
    value ? t('comments.settings.on') : t('comments.settings.off');

  return (
    <section className={styles.settings} aria-label={t('comments.settings.title')}>
      <div className={styles.settingsHead}>
        <strong>{t('comments.settings.title')}</strong>
        {!isAdmin && <span className={shop.muted}>{t('common.requiresAdmin')}</span>}
        {isAdmin && changed && (
          <Button
            variant="tool"
            disabled={saving}
            onClick={() => save({ require_approval: null, auto_approve_known: null })}
          >
            {t('comments.settings.reset')}
          </Button>
        )}
      </div>
      <label className={styles.toggle}>
        <input
          type="checkbox"
          checked={policy.require_approval}
          disabled={disabled}
          onChange={(e) => save({ ...overrides, require_approval: e.target.checked })}
        />
        <span>
          {t('comments.settings.requireApproval')}
          <span className={styles.toggleHint}>
            {t('comments.settings.default', { value: onOff(defaults.require_approval) })}
          </span>
        </span>
      </label>
      <label className={cx(styles.toggle, !policy.require_approval && styles.inactive)}>
        <input
          type="checkbox"
          checked={policy.auto_approve_known}
          disabled={disabled || !policy.require_approval}
          onChange={(e) => save({ ...overrides, auto_approve_known: e.target.checked })}
        />
        <span>
          {t('comments.settings.autoApproveKnown')}
          <span className={styles.toggleHint}>
            {t('comments.settings.default', { value: onOff(defaults.auto_approve_known) })}
          </span>
        </span>
      </label>
    </section>
  );
};

interface ReplyModalProps {
  comment: AdminComment;
  onClose: () => void;
  onReplied: () => void;
  onError: (message: string) => void;
}

const ReplyModal: React.FC<ReplyModalProps> = ({ comment, onClose, onReplied, onError }) => {
  const { t } = useTranslation();
  const [body, setBody] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!body.trim()) return;
    setBusy(true);
    try {
      await aegisApi.replyToComment(comment.id, body);
      onReplied();
    } catch (err) {
      onError(messageOf(err, t('api.replyCommentFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('comments.replyTitle', { name: comment.author_name })}
      icon={<Reply size={18} />}
      onClose={onClose}
    >
      <form onSubmit={handleSubmit} className={shop.detail}>
        <div className={shop.box}>
          <div className={shop.boxTitle}>
            {comment.author_name} · {formatDateTime(comment.created_at)}
          </div>
          {comment.body}
        </div>
        {comment.status === 'pending' && (
          <div className={shop.notice}>{t('comments.replyApproves')}</div>
        )}
        <textarea
          className={styles.replyInput}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          aria-label={t('comments.replyLabel')}
          placeholder={t('comments.replyPlaceholder')}
          rows={6}
          autoFocus
        />
        <div className={shop.actionsBar} style={{ justifyContent: 'flex-end' }}>
          <Button type="button" variant="tool" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" variant="primary" disabled={busy || !body.trim()}>
            <Reply size={14} />
            <span>{t('comments.actions.reply')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
