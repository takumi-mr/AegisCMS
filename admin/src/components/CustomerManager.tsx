import React, { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { AdminCustomer, CustomerDetail, CustomerPage, CustomerStatus } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { formatMoney } from '../utils/money';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

const PAGE_SIZE = 25;

const STATUS_BADGE: Record<CustomerStatus, string | undefined> = {
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

interface CustomerManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** The shop's customer accounts for editors: who signed up, their orders, and suspending an account */
export const CustomerManager: React.FC<CustomerManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<CustomerPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<AdminCustomer | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listCustomers({ limit: PAGE_SIZE, offset })
      .then((loaded) => {
        if (cancelled) return;
        setPage(loaded);
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.status === 404
            ? i18n.t('customers.disabled')
            : messageOf(err, i18n.t('customers.loadFailed'))
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
            <h1>{t('customers.title')}</h1>
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
                  <th>{t('customers.columns.customer')}</th>
                  <th>{t('customers.columns.status')}</th>
                  <th>{t('customers.columns.registered')}</th>
                  <th>{t('customers.columns.lastLogin')}</th>
                </tr>
              </thead>
              <tbody>
                {page.customers.map((customer) => (
                  <tr
                    key={customer.id}
                    className={styles.clickable}
                    onClick={() => setSelected(customer)}
                  >
                    <td>
                      <div className={styles.productNames}>
                        <span>{customer.name || '—'}</span>
                        <span className={styles.muted}>{customer.email}</span>
                      </div>
                    </td>
                    <td>
                      <span className={cx(styles.badge, STATUS_BADGE[customer.status])}>
                        {t(`customers.status.${customer.status}`)}
                      </span>
                    </td>
                    <td className={styles.muted}>{formatDate(customer.created_at)}</td>
                    <td className={styles.muted}>{formatDate(customer.last_login_at)}</td>
                  </tr>
                ))}
                {page.customers.length === 0 && (
                  <tr>
                    <td colSpan={4} className={styles.empty}>
                      {t('customers.empty')}
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
              <span>{t('customers.newer')}</span>
            </Button>
            <span>
              {t('customers.range', {
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
              <span>{t('customers.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {selected && (
        <CustomerModal
          customer={selected}
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

interface CustomerModalProps {
  customer: AdminCustomer;
  onClose: () => void;
  onChanged: (message: string) => void;
  onError: (message: string) => void;
}

/** One customer: registration details, the orders, and suspend / reactivate */
const CustomerModal: React.FC<CustomerModalProps> = ({
  customer: listed,
  onClose,
  onChanged,
  onError,
}) => {
  const { t } = useTranslation();
  const [detail, setDetail] = useState<CustomerDetail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  // The status the dialog shows after a change (the list behind it reloads separately)
  const [status, setStatus] = useState<CustomerStatus>(listed.status);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getCustomer(listed.id)
      .then((loaded) => {
        if (cancelled) return;
        setDetail(loaded);
        setStatus(loaded.customer.status);
      })
      .catch((err: unknown) => {
        if (!cancelled) setLoadError(messageOf(err, i18n.t('customers.loadFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [listed.id]);

  const customer = detail?.customer ?? listed;
  const address = customer.shipping_address;
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
    if (next === 'suspended' && !window.confirm(t('customers.confirmSuspend'))) return;
    setBusy(true);
    try {
      await aegisApi.setCustomerStatus(customer.id, next);
      setStatus(next);
      onChanged(t(next === 'suspended' ? 'customers.suspended' : 'customers.reactivated'));
    } catch (err) {
      onError(messageOf(err, t('api.setCustomerStatusFailed')));
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
    <Modal
      title={customer.name || customer.email}
      icon={<Users size={18} />}
      onClose={onClose}
      flush
    >
      <div className={styles.detail}>
        <div className={styles.actionsBar}>
          <span className={cx(styles.badge, STATUS_BADGE[status])}>
            {t(`customers.status.${status}`)}
          </span>
          <span className={styles.muted}>
            {t('customers.registeredAt', { date: formatDate(customer.created_at) })}
          </span>
          {status === 'active' && (
            <Button variant="tool" danger disabled={busy} onClick={() => change('suspended')}>
              {t('customers.suspend')}
            </Button>
          )}
          {status === 'suspended' && (
            <Button variant="tool" disabled={busy} onClick={() => change('active')}>
              {t('customers.reactivate')}
            </Button>
          )}
        </div>

        {status === 'unverified' && (
          <div className={styles.notice}>{t('customers.unverified')}</div>
        )}

        <div className={styles.detailGrid}>
          <div className={styles.box}>
            <div className={styles.boxTitle}>{t('customers.contact')}</div>
            {customer.name || '—'}
            {'\n'}
            <a href={`mailto:${customer.email}`}>{customer.email}</a>
            {customer.phone ? `\n${customer.phone}` : ''}
          </div>
          <div className={styles.box}>
            <div className={styles.boxTitle}>{t('customers.shipTo')}</div>
            {addressText || <span className={styles.muted}>{t('customers.noAddress')}</span>}
          </div>
        </div>

        <div className={styles.box} style={{ whiteSpace: 'normal' }}>
          <div className={styles.boxTitle}>{t('customers.orders')}</div>
          {loadError && <div className={cx(styles.notice, styles.error)}>{loadError}</div>}
          {!detail && !loadError && <div className={styles.muted}>{t('common.loading')}</div>}
          {detail && detail.orders.length === 0 && (
            <div className={styles.muted}>{t('customers.noOrders')}</div>
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
              <span>{t('customers.spent')}</span>
              <span>{formatMoney(amount, currency)}</span>
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
};
