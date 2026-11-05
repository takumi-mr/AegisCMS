import React, { useEffect, useState } from 'react';
import {
  Check,
  ChevronLeft,
  ChevronRight,
  RefreshCw,
  Reply,
  ShieldAlert,
  Star,
  Trash2,
  Undo2,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { AdminReview, ReviewBulkAction, ReviewPage, ReviewStatus } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import shop from './Shop.module.css';
import styles from './CommentManager.module.css';

const PAGE_SIZE = 25;

const FILTERS: (ReviewStatus | '')[] = ['pending', 'approved', 'trash', 'spam', ''];

const STATUS_BADGE: Record<ReviewStatus, string | undefined> = {
  pending: shop.warn,
  approved: shop.good,
  spam: shop.bad,
  trash: shop.bad,
};

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

const stars = (rating: number): string => '★'.repeat(rating) + '☆'.repeat(Math.max(0, 5 - rating));

interface ReviewManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** A pending move that needs a reason first (rejecting or marking as spam) */
interface PendingReason {
  ids: string[];
  action: 'trash' | 'spam';
}

/** Product reviews for editors: approve, hold, reject, mark as spam, delete and reply as the shop */
export const ReviewManager: React.FC<ReviewManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<ReviewStatus | ''>('pending');
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<ReviewPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<boolean>(false);
  const [replyTo, setReplyTo] = useState<AdminReview | null>(null);
  const [askReason, setAskReason] = useState<PendingReason | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listReviews({ status, limit: PAGE_SIZE, offset })
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
            ? i18n.t('reviews.disabled')
            : messageOf(err, i18n.t('reviews.loadFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, offset, reloadCount]);

  const reload = () => setReloadCount((n) => n + 1);

  const run = async (ids: string[], action: ReviewBulkAction, reason?: string) => {
    if (ids.length === 0) return;
    setBusy(true);
    try {
      await aegisApi.bulkReviews(ids, action, reason);
      onShowToast(t(`reviews.done.${action}`, { count: ids.length }));
      reload();
    } catch (err) {
      onShowToast(messageOf(err, t('api.moderateReviewFailed')), 'error');
    } finally {
      setBusy(false);
    }
  };

  const apply = (ids: string[], action: ReviewBulkAction) => {
    if (ids.length === 0) return;
    if (action === 'delete') {
      if (!window.confirm(t('reviews.confirmDelete', { count: ids.length }))) return;
    } else if (action === 'trash' || action === 'spam') {
      // Rejecting asks for a reason (optional); it is kept with the review
      setAskReason({ ids, action });
      return;
    }
    void run(ids, action);
  };

  const reviews = page?.reviews ?? [];
  const allSelected = reviews.length > 0 && reviews.every((r) => selected.has(r.id));
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
            <Star size={22} />
            <h1>{t('reviews.title')}</h1>
            {page && <span className={shop.count}>{page.total}</span>}
          </div>
          <div className={shop.headingActions}>
            <Button variant="tool" onClick={reload}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        <div className={shop.filters} role="tablist" aria-label={t('reviews.statusFilter')}>
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
              {value ? t(`reviews.status.${value}`) : t('reviews.all')}
            </button>
          ))}
        </div>

        {error && <div className={cx(shop.notice, shop.error)}>{error}</div>}

        {page && selectedIds.length > 0 && (
          <div className={styles.bulkBar} role="toolbar" aria-label={t('reviews.bulkActions')}>
            <span>{t('reviews.selected', { count: selectedIds.length })}</span>
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
                        setSelected(allSelected ? new Set() : new Set(reviews.map((r) => r.id)))
                      }
                      aria-label={t('reviews.selectAll')}
                    />
                  </th>
                  <th>{t('reviews.columns.author')}</th>
                  <th>{t('reviews.columns.review')}</th>
                  <th>{t('reviews.columns.product')}</th>
                  <th>{t('reviews.columns.status')}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {reviews.map((review) => {
                  const name = review.author_name || t('reviews.anonymous');
                  return (
                    <tr key={review.id}>
                      <td className={styles.checkCell}>
                        <input
                          type="checkbox"
                          checked={selected.has(review.id)}
                          onChange={() => toggle(review.id)}
                          aria-label={t('reviews.selectOne', { name })}
                        />
                      </td>
                      <td>
                        <div className={shop.productNames}>
                          <span>{name}</span>
                          {review.author_email && (
                            <a className={styles.email} href={`mailto:${review.author_email}`}>
                              {review.author_email}
                            </a>
                          )}
                          {review.verified_purchase === 1 && (
                            <span className={cx(shop.badge, shop.good)}>
                              {t('reviews.verified')}
                            </span>
                          )}
                        </div>
                      </td>
                      <td className={styles.commentCell}>
                        <div className={shop.muted}>
                          <span aria-label={`${review.rating} / 5`}>{stars(review.rating)}</span> ·{' '}
                          {formatDateTime(review.created_at)}
                        </div>
                        {review.title && <strong>{review.title}</strong>}
                        <p className={styles.body}>{review.body}</p>
                        {review.reply && (
                          <div className={shop.box}>
                            <div className={shop.boxTitle}>{t('reviews.reply')}</div>
                            {review.reply}
                          </div>
                        )}
                        {review.rejection_reason && (
                          <div className={shop.muted}>
                            {t('reviews.reason', { reason: review.rejection_reason })}
                          </div>
                        )}
                      </td>
                      <td className={cx(shop.muted, shop.mono)}>{review.product_id}</td>
                      <td>
                        <span className={cx(shop.badge, STATUS_BADGE[review.status])}>
                          {t(`reviews.status.${review.status}`)}
                        </span>
                      </td>
                      <td>
                        <div className={shop.rowActions}>
                          <Button
                            variant="icon"
                            disabled={busy}
                            onClick={() => setReplyTo(review)}
                            title={t('reviews.actions.reply')}
                            aria-label={t('reviews.actions.reply')}
                          >
                            <Reply size={14} />
                          </Button>
                          <StatusActions
                            status={review.status}
                            disabled={busy}
                            compact
                            onAction={(action) => apply([review.id], action)}
                          />
                        </div>
                      </td>
                    </tr>
                  );
                })}
                {reviews.length === 0 && (
                  <tr>
                    <td colSpan={6} className={shop.empty}>
                      {status
                        ? t('reviews.emptyStatus', { status: t(`reviews.status.${status}`) })
                        : t('reviews.empty')}
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
              <span>{t('reviews.newer')}</span>
            </Button>
            <span>
              {t('reviews.range', {
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
              <span>{t('reviews.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {askReason && (
        <ReasonModal
          pending={askReason}
          onClose={() => setAskReason(null)}
          onConfirm={(reason) => {
            const { ids, action } = askReason;
            setAskReason(null);
            void run(ids, action, reason);
          }}
        />
      )}

      {replyTo && (
        <ReplyModal
          review={replyTo}
          onClose={() => setReplyTo(null)}
          onDone={(message) => {
            setReplyTo(null);
            onShowToast(message);
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
  status?: ReviewStatus;
  disabled: boolean;
  /** Icon buttons (rows) instead of labelled buttons (bulk bar) */
  compact?: boolean;
  onAction: (action: ReviewBulkAction) => void;
}

/** The moves that make sense from a status: approve / hold, reject, spam, delete */
const StatusActions: React.FC<StatusActionsProps> = ({ status, disabled, compact, onAction }) => {
  const { t } = useTranslation();
  const actions: { action: ReviewBulkAction; icon: React.ReactNode; danger?: boolean }[] = [];
  if (status !== 'approved') actions.push({ action: 'approve', icon: <Check size={14} /> });
  if (status === 'approved') actions.push({ action: 'pending', icon: <Undo2 size={14} /> });
  if (status !== 'trash') actions.push({ action: 'trash', icon: <X size={14} /> });
  if (status !== 'spam') actions.push({ action: 'spam', icon: <ShieldAlert size={14} /> });
  actions.push({ action: 'delete', icon: <Trash2 size={14} />, danger: true });
  return (
    <>
      {actions.map(({ action, icon, danger }) => {
        const label = t(`reviews.actions.${action}`);
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

interface ReasonModalProps {
  pending: PendingReason;
  onClose: () => void;
  onConfirm: (reason: string) => void;
}

/** Asks why reviews are rejected or marked as spam; the reason is optional and only kept for the shop */
const ReasonModal: React.FC<ReasonModalProps> = ({ pending, onClose, onConfirm }) => {
  const { t } = useTranslation();
  const [reason, setReason] = useState<string>('');
  const title = t(pending.action === 'spam' ? 'reviews.spamTitle' : 'reviews.rejectTitle', {
    count: pending.ids.length,
  });

  return (
    <Modal title={title} icon={<X size={18} />} onClose={onClose} flush>
      <form
        className={shop.detail}
        onSubmit={(e) => {
          e.preventDefault();
          onConfirm(reason.trim());
        }}
      >
        <textarea
          className={styles.replyInput}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          aria-label={t('reviews.reasonLabel')}
          rows={4}
          autoFocus
        />
        <span className={shop.hint}>{t('reviews.reasonHint')}</span>
        <div className={shop.actionsBar} style={{ justifyContent: 'flex-end' }}>
          <Button type="button" variant="tool" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" variant="primary">
            {t(`reviews.actions.${pending.action}`)}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

interface ReplyModalProps {
  review: AdminReview;
  onClose: () => void;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}

/** The shop's one reply under a review: write it, change it or remove it */
const ReplyModal: React.FC<ReplyModalProps> = ({ review, onClose, onDone, onError }) => {
  const { t } = useTranslation();
  const [body, setBody] = useState<string>(review.reply ?? '');
  const [busy, setBusy] = useState<boolean>(false);
  const name = review.author_name || t('reviews.anonymous');
  const changed = body.trim() !== (review.reply ?? '').trim();

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!body.trim() || !changed) return;
    setBusy(true);
    try {
      await aegisApi.setReviewReply(review.id, body.trim());
      onDone(t('reviews.replySaved'));
    } catch (err) {
      onError(messageOf(err, t('api.replyReviewFailed')));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    try {
      await aegisApi.deleteReviewReply(review.id);
      onDone(t('reviews.replyRemoved'));
    } catch (err) {
      onError(messageOf(err, t('api.replyReviewFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('reviews.replyTitle', { name })}
      icon={<Reply size={18} />}
      onClose={onClose}
      flush
    >
      <form onSubmit={save} className={shop.detail}>
        <div className={shop.box}>
          <div className={shop.boxTitle}>
            {stars(review.rating)} · {name} · {formatDateTime(review.created_at)}
          </div>
          {review.title ? `${review.title}\n` : ''}
          {review.body}
        </div>
        <textarea
          className={styles.replyInput}
          value={body}
          onChange={(e) => setBody(e.target.value)}
          aria-label={t('reviews.replyLabel')}
          placeholder={t('reviews.replyPlaceholder')}
          rows={6}
          autoFocus
        />
        <div className={shop.actionsBar} style={{ justifyContent: 'flex-end' }}>
          {review.reply && (
            <Button type="button" variant="tool" danger disabled={busy} onClick={remove}>
              {t('reviews.replyRemove')}
            </Button>
          )}
          <Button type="button" variant="tool" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" variant="primary" disabled={busy || !body.trim() || !changed}>
            <Reply size={14} />
            <span>{t('reviews.replySave')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
