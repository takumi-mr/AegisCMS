import React, { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { AdminMember, MemberDetail, MemberPage, MemberStatus } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { formatMoney } from '../utils/money';
import { Avatar } from './ui/Avatar';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

const PAGE_SIZE = 25;

const STATUS_BADGE: Record<MemberStatus, string | undefined> = {
  active: styles.good,
  suspended: styles.bad,
  unverified: styles.warn,
};

const ORDER_BADGE: Record<string, string | undefined> = {
  pending_payment: styles.warn,
  paid: styles.good,
  shipped: styles.good,
  completed: undefined,
  cancelled: styles.bad,
  refunded: styles.bad,
};

const formatDate = (iso: string | null): string => (iso ? formatDateTime(iso) : '—');

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface MemberManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
  /** A member to open at once (the link from the forum); `onOpened` is called once it is taken */
  openMemberId?: string | null;
  onOpened?: () => void;
  /** The shop is on: the member detail lists the orders (member accounts also work without a shop) */
  showOrders?: boolean;
  /** The signed-in user is an admin: the access level can be changed (editors can only see it) */
  canGrantLevel?: boolean;
}

/** The shop's member accounts for editors: who signed up, their orders, and suspending an account */
export const MemberManager: React.FC<MemberManagerProps> = ({
  onShowToast,
  openMemberId,
  onOpened,
  showOrders = true,
  canGrantLevel = false,
}) => {
  const { t } = useTranslation();
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<MemberPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<AdminMember | null>(null);

  // Opened from elsewhere (the forum): look the member up, then show the same dialog as a list click
  useEffect(() => {
    if (!openMemberId) return;
    let cancelled = false;
    aegisApi
      .getMember(openMemberId)
      .then((loaded) => {
        if (!cancelled) setSelected(loaded.member);
      })
      .catch((err: unknown) => {
        if (!cancelled) onShowToast(messageOf(err, i18n.t('members.loadFailed')), 'error');
      })
      .finally(() => {
        if (!cancelled) onOpened?.();
      });
    return () => {
      cancelled = true;
    };
    // onShowToast / onOpened change on every render of the parent; only the id starts a lookup
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openMemberId]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listMembers({ limit: PAGE_SIZE, offset })
      .then((loaded) => {
        if (cancelled) return;
        setPage(loaded);
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.status === 404
            ? i18n.t('members.disabled')
            : messageOf(err, i18n.t('members.loadFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [offset, reloadCount]);

  const reload = () => setReloadCount((n) => n + 1);

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <Users size={22} />
            <h1>{t('members.title')}</h1>
            {page && <span className={styles.count}>{page.total}</span>}
          </div>
          <div className={styles.headingActions}>
            <Button variant="tool" onClick={reload}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {page && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('members.columns.member')}</th>
                  <th>{t('members.columns.status')}</th>
                  <th>{t('members.columns.level')}</th>
                  <th>{t('members.columns.registered')}</th>
                  <th>{t('members.columns.lastLogin')}</th>
                </tr>
              </thead>
              <tbody>
                {page.members.map((member) => (
                  <tr
                    key={member.id}
                    className={styles.clickable}
                    onClick={() => setSelected(member)}
                  >
                    <td>
                      <div className={styles.productNames}>
                        <span>{member.name || '—'}</span>
                        <span className={styles.muted}>{member.email}</span>
                      </div>
                    </td>
                    <td>
                      <span className={cx(styles.badge, STATUS_BADGE[member.status])}>
                        {t(`members.status.${member.status}`)}
                      </span>
                    </td>
                    <td className={styles.muted}>{member.level}</td>
                    <td className={styles.muted}>{formatDate(member.created_at)}</td>
                    <td className={styles.muted}>{formatDate(member.last_login_at)}</td>
                  </tr>
                ))}
                {page.members.length === 0 && (
                  <tr>
                    <td colSpan={5} className={styles.empty}>
                      {t('members.empty')}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}

        {page && page.total > PAGE_SIZE && (
          <div className={styles.pagination}>
            <Button
              variant="tool"
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
            >
              <ChevronLeft size={14} />
              <span>{t('members.newer')}</span>
            </Button>
            <span>
              {t('members.range', {
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
              <span>{t('members.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {selected && (
        <MemberModal
          member={selected}
          showOrders={showOrders}
          canGrantLevel={canGrantLevel}
          onClose={() => setSelected(null)}
          onChanged={(message) => {
            onShowToast(message);
            reload();
          }}
          onError={(message) => onShowToast(message, 'error')}
        />
      )}
    </main>
  );
};

interface MemberModalProps {
  member: AdminMember;
  showOrders: boolean;
  canGrantLevel?: boolean;
  onClose: () => void;
  onChanged: (message: string) => void;
  onError: (message: string) => void;
}

/** One member: registration details, the orders, and suspend / reactivate */
const MemberModal: React.FC<MemberModalProps> = ({
  member: listed,
  showOrders,
  canGrantLevel,
  onClose,
  onChanged,
  onError,
}) => {
  const { t } = useTranslation();
  const [detail, setDetail] = useState<MemberDetail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  // The status the dialog shows after a change (the list behind it reloads separately)
  const [status, setStatus] = useState<MemberStatus>(listed.status);
  const [level, setLevel] = useState<string>(listed.level);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getMember(listed.id)
      .then((loaded) => {
        if (cancelled) return;
        setDetail(loaded);
        setStatus(loaded.member.status);
        setLevel(loaded.member.level);
      })
      .catch((err: unknown) => {
        if (!cancelled) setLoadError(messageOf(err, i18n.t('members.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [listed.id]);

  const member = detail?.member ?? listed;
  const address = member.shipping_address;
  const addressText = address
    ? [
        address.name,
        address.postal_code,
        `${address.region} ${address.city}`.trim(),
        address.line1,
        address.line2,
        address.phone,
      ]
        .filter((part) => part && part.trim())
        .join('\n')
    : '';

  const change = async (next: 'active' | 'suspended') => {
    if (next === 'suspended' && !window.confirm(t('members.confirmSuspend'))) return;
    setBusy(true);
    try {
      await aegisApi.setMemberStatus(member.id, next);
      setStatus(next);
      onChanged(t(next === 'suspended' ? 'members.suspended' : 'members.reactivated'));
    } catch (err) {
      onError(messageOf(err, t('api.setMemberStatusFailed')));
    } finally {
      setBusy(false);
    }
  };

  const changeLevel = async (next: string) => {
    setBusy(true);
    try {
      await aegisApi.setMemberLevel(member.id, next);
      setLevel(next);
      onChanged(t('members.levelChanged', { level: next }));
    } catch (err) {
      onError(messageOf(err, t('api.setMemberLevelFailed')));
    } finally {
      setBusy(false);
    }
  };

  // Spent per currency: an order that is unpaid, cancelled or refunded is not spending
  const spent = new Map<string, number>();
  for (const order of detail?.orders ?? []) {
    if (order.status === 'paid' || order.status === 'shipped' || order.status === 'completed') {
      spent.set(order.currency, (spent.get(order.currency) ?? 0) + order.total);
    }
  }

  return (
    <Modal title={member.name || member.email} icon={<Users size={18} />} onClose={onClose} flush>
      <div className={styles.detail}>
        <div className={styles.actionsBar}>
          <span className={cx(styles.badge, STATUS_BADGE[status])}>
            {t(`members.status.${status}`)}
          </span>
          <span className={styles.muted}>
            {t('members.registeredAt', { date: formatDate(member.created_at) })}
          </span>
          {status === 'active' && (
            <Button variant="tool" danger disabled={busy} onClick={() => change('suspended')}>
              {t('members.suspend')}
            </Button>
          )}
          {status === 'suspended' && (
            <Button variant="tool" disabled={busy} onClick={() => change('active')}>
              {t('members.reactivate')}
            </Button>
          )}
        </div>

        <div className={styles.actionsBar}>
          <label htmlFor="member-level">{t('members.level')}</label>
          {canGrantLevel && detail ? (
            <select
              id="member-level"
              className={styles.statusFilter}
              value={level}
              disabled={busy}
              onChange={(e) => void changeLevel(e.target.value)}
            >
              {/* A level removed from [members].levels stays visible until it is changed */}
              {!detail.levels.includes(level) && <option value={level}>{level}</option>}
              {detail.levels.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          ) : (
            <span className={styles.badge}>{level}</span>
          )}
        </div>

        {status === 'unverified' && <div className={styles.notice}>{t('members.unverified')}</div>}

        <div className={styles.detailGrid}>
          <div className={styles.box}>
            <div className={styles.boxTitle}>{t('members.contact')}</div>
            <Avatar
              url={member.avatar_url}
              name={member.nickname || member.name || member.email}
              size={40}
            />
            {'\n'}
            {member.name || '—'}
            {'\n'}
            <a href={`mailto:${member.email}`}>{member.email}</a>
            {member.nickname ? `\n${t('members.nickname', { name: member.nickname })}` : ''}
            {member.phone ? `\n${member.phone}` : ''}
          </div>
          <div className={styles.box}>
            <div className={styles.boxTitle}>{t('members.shipTo')}</div>
            {addressText || <span className={styles.muted}>{t('members.noAddress')}</span>}
          </div>
        </div>

        {detail?.forum && (
          <div className={styles.box} style={{ whiteSpace: 'normal' }}>
            <div className={styles.boxTitle}>{t('members.forum')}</div>
            <div className={styles.sumRow}>
              <span>{t('members.forumThreads')}</span>
              <span>{detail.forum.threads}</span>
            </div>
            <div className={styles.sumRow}>
              <span>{t('members.forumPosts')}</span>
              <span>{detail.forum.posts}</span>
            </div>
          </div>
        )}

        {showOrders && (
          <div className={styles.box} style={{ whiteSpace: 'normal' }}>
            <div className={styles.boxTitle}>{t('members.orders')}</div>
            {loadError && <div className={cx(styles.notice, styles.error)}>{loadError}</div>}
            {!detail && !loadError && <div className={styles.muted}>{t('common.loading')}</div>}
            {detail && detail.orders.length === 0 && (
              <div className={styles.muted}>{t('members.noOrders')}</div>
            )}
            {detail?.orders.map((order) => (
              <div key={order.id} className={styles.sumRow}>
                <span>
                  <span className={styles.mono}>{order.number}</span>{' '}
                  <span className={styles.muted}>{formatDate(order.created_at)}</span>
                </span>
                <span>
                  <span className={cx(styles.badge, ORDER_BADGE[order.status])}>
                    {t(`orders.status.${order.status}`)}
                  </span>{' '}
                  {formatMoney(order.total, order.currency)}
                </span>
              </div>
            ))}
            {[...spent].map(([currency, amount]) => (
              <div key={currency} className={cx(styles.sumRow, styles.sumTotal)}>
                <span>{t('members.spent')}</span>
                <span>{formatMoney(amount, currency)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
};
