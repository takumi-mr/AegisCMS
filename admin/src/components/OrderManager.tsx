import React, { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, RotateCcw, ShoppingBag } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { Order, OrderPage, OrderStatus } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { formatMoney } from '../utils/money';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

const PAGE_SIZE = 25;

const STATUS_BADGE: Record<OrderStatus, string | undefined> = {
  pending_payment: styles.warn,
  paid: styles.good,
  shipped: styles.good,
  completed: undefined,
  cancelled: styles.bad,
  refunded: styles.bad,
};

const FILTERS: (OrderStatus | '')[] = [
  '',
  'pending_payment',
  'paid',
  'shipped',
  'completed',
  'cancelled',
  'refunded',
];

const formatDate = (iso: string | null): string => (iso ? formatDateTime(iso) : '—');

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface OrderManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Shop orders for editors: filter by status, open an order and move it along */
export const OrderManager: React.FC<OrderManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<OrderStatus | ''>('');
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<OrderPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Order | null>(null);
  const [today] = useState(() => new Date().toISOString().slice(0, 10));
  const [reportStart, setReportStart] = useState(() =>
    new Date(Date.now() - 29 * 86400000).toISOString().slice(0, 10)
  );
  const [reportEnd, setReportEnd] = useState(today);
  const [report, setReport] = useState<Awaited<ReturnType<typeof aegisApi.getSalesReport>> | null>(
    null
  );
  const [reportError, setReportError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listOrders({ status, limit: PAGE_SIZE, offset })
      .then((loaded) => {
        if (cancelled) return;
        setPage(loaded);
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(
          err instanceof ApiError && err.status === 404
            ? i18n.t('orders.shopDisabled')
            : messageOf(err, i18n.t('orders.loadFailed'))
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, offset, reloadCount]);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getSalesReport(reportStart, reportEnd)
      .then((loaded) => {
        if (cancelled) return;
        setReport(loaded);
        setReportError(null);
      })
      .catch((err: unknown) => {
        if (!cancelled) setReportError(messageOf(err, i18n.t('orders.reportFailed')));
      });
    return () => {
      cancelled = true;
    };
  }, [reportStart, reportEnd, reloadCount]);

  const handleUpdated = (updated: Order) => {
    setSelected(updated);
    setPage((prev) =>
      prev ? { ...prev, orders: prev.orders.map((o) => (o.id === updated.id ? updated : o)) } : prev
    );
    onShowToast(
      t('orders.updated', {
        number: updated.number,
        status: t(`orders.status.${updated.status}`),
      })
    );
  };

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <ShoppingBag size={22} />
            <h1>{t('orders.title')}</h1>
            {page && <span className={styles.count}>{page.total}</span>}
          </div>
          <div className={styles.headingActions}>
            <Button variant="tool" onClick={() => setReloadCount((n) => n + 1)}>
              <RefreshCw size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        <section className={styles.report} aria-labelledby="sales-report-title">
          <div className={styles.reportHeading}>
            <h2 id="sales-report-title">{t('orders.reportTitle')}</h2>
            <div className={styles.reportDates}>
              <label>
                <span>{t('orders.reportFrom')}</span>
                <input
                  type="date"
                  value={reportStart}
                  max={reportEnd}
                  onChange={(e) => setReportStart(e.target.value)}
                />
              </label>
              <label>
                <span>{t('orders.reportTo')}</span>
                <input
                  type="date"
                  value={reportEnd}
                  min={reportStart}
                  max={today}
                  onChange={(e) => setReportEnd(e.target.value)}
                />
              </label>
            </div>
          </div>
          {reportError && <div className={cx(styles.notice, styles.error)}>{reportError}</div>}
          {report && (
            <div className={styles.tableWrap}>
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th>{t('orders.reportDate')}</th>
                    <th>{t('orders.reportCurrency')}</th>
                    <th className={styles.numeric}>{t('orders.reportOrders')}</th>
                    <th className={styles.numeric}>{t('orders.reportGross')}</th>
                    <th className={styles.numeric}>{t('orders.reportRefunded')}</th>
                    <th className={styles.numeric}>{t('orders.reportNet')}</th>
                  </tr>
                </thead>
                <tbody>
                  {report.days.map((row) => (
                    <tr key={`${row.day}-${row.currency}`}>
                      <td>{row.day}</td>
                      <td>{row.currency.toUpperCase()}</td>
                      <td className={styles.numeric}>{row.orders}</td>
                      <td className={styles.numeric}>{formatMoney(row.gross, row.currency)}</td>
                      <td className={styles.numeric}>{formatMoney(row.refunded, row.currency)}</td>
                      <td className={styles.numeric}>
                        {formatMoney(row.gross - row.refunded, row.currency)}
                      </td>
                    </tr>
                  ))}
                  {report.days.length === 0 && (
                    <tr>
                      <td colSpan={6} className={styles.empty}>
                        {t('orders.reportEmpty')}
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          )}
        </section>

        <div className={styles.filters} role="tablist" aria-label={t('orders.statusFilter')}>
          {FILTERS.map((value) => (
            <button
              key={value || 'all'}
              type="button"
              role="tab"
              aria-selected={status === value}
              className={cx(styles.filter, status === value && styles.active)}
              onClick={() => {
                setStatus(value);
                setOffset(0);
              }}
            >
              {value ? t(`orders.status.${value}`) : t('orders.all')}
            </button>
          ))}
        </div>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {page && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('orders.columns.order')}</th>
                  <th>{t('orders.columns.placed')}</th>
                  <th>{t('orders.columns.customer')}</th>
                  <th>{t('orders.columns.items')}</th>
                  <th className={styles.numeric}>{t('orders.columns.total')}</th>
                  <th>{t('orders.columns.status')}</th>
                </tr>
              </thead>
              <tbody>
                {page.orders.map((order) => (
                  <tr
                    key={order.id}
                    className={styles.clickable}
                    onClick={() => setSelected(order)}
                  >
                    <td className={styles.mono}>{order.number}</td>
                    <td className={styles.muted}>{formatDate(order.created_at)}</td>
                    <td>
                      <div className={styles.productNames}>
                        <span>{order.customer_name || '—'}</span>
                        <span className={styles.muted}>{order.customer_email}</span>
                      </div>
                    </td>
                    <td className={styles.muted}>
                      {order.lines.reduce((sum, line) => sum + line.quantity, 0)}
                    </td>
                    <td className={styles.numeric}>{formatMoney(order.total, order.currency)}</td>
                    <td>
                      <span className={cx(styles.badge, STATUS_BADGE[order.status])}>
                        {t(`orders.status.${order.status}`)}
                      </span>
                    </td>
                  </tr>
                ))}
                {page.orders.length === 0 && (
                  <tr>
                    <td colSpan={6} className={styles.empty}>
                      {status
                        ? t('orders.emptyStatus', { status: t(`orders.status.${status}`) })
                        : t('orders.empty')}
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
              <span>{t('orders.newer')}</span>
            </Button>
            <span>
              {t('orders.range', {
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
              <span>{t('orders.older')}</span>
              <ChevronRight size={14} />
            </Button>
          </div>
        )}
      </div>

      {selected && (
        <OrderDetail
          order={selected}
          onUpdated={handleUpdated}
          onError={(message) => onShowToast(message, 'error')}
          onSuccess={(message) => onShowToast(message, 'success')}
          onClose={() => setSelected(null)}
        />
      )}
    </main>
  );
};

interface OrderDetailProps {
  order: Order;
  onUpdated: (order: Order) => void;
  onError: (message: string) => void;
  onSuccess: (message: string) => void;
  onClose: () => void;
}

const OrderDetail: React.FC<OrderDetailProps> = ({
  order,
  onUpdated,
  onError,
  onSuccess,
  onClose,
}) => {
  const { t } = useTranslation();
  const [note, setNote] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);
  const [showRefund, setShowRefund] = useState<boolean>(false);

  const move = async (status: OrderStatus) => {
    if (status === 'cancelled') {
      const refund = order.status === 'paid' ? t('orders.noRefund') : '';
      if (!window.confirm(t('orders.confirmCancel', { number: order.number, refund }))) return;
    }
    setBusy(true);
    try {
      onUpdated(await aegisApi.updateOrderStatus(order.id, status, note.trim() || undefined));
      setNote('');
    } catch (err) {
      onError(messageOf(err, t('api.updateOrderFailed')));
    } finally {
      setBusy(false);
    }
  };

  const address = order.shipping_address;
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

  return (
    <>
      <Modal
        title={t('orders.detailTitle', { number: order.number })}
        icon={<ShoppingBag size={18} />}
        onClose={onClose}
        wide
        flush
      >
        <div className={styles.detail}>
          <div className={styles.actionsBar}>
            <span className={cx(styles.badge, STATUS_BADGE[order.status])}>
              {t(`orders.status.${order.status}`)}
            </span>
            <span className={styles.muted}>
              {t('orders.placedAt', { date: formatDate(order.created_at) })}
            </span>
            {order.paid_at && (
              <span className={styles.muted}>
                {t('orders.paidAt', { date: formatDate(order.paid_at) })}
              </span>
            )}
            {order.status === 'pending_payment' && order.expires_at && (
              <span className={styles.muted}>
                {t('orders.heldUntil', { date: formatDate(order.expires_at) })}
              </span>
            )}
          </div>

          <div className={styles.detailGrid}>
            <div className={styles.box}>
              <div className={styles.boxTitle}>{t('orders.customer')}</div>
              {order.customer_name || '—'}
              {'\n'}
              <a href={`mailto:${order.customer_email}`}>{order.customer_email}</a>
            </div>
            <div className={styles.box}>
              <div className={styles.boxTitle}>{t('orders.shipTo')}</div>
              {addressText || <span className={styles.muted}>{t('orders.noAddress')}</span>}
            </div>
          </div>

          <div className={styles.box} style={{ whiteSpace: 'normal' }}>
            <div className={styles.boxTitle}>{t('orders.items')}</div>
            {order.lines.map((line) => (
              <div key={line.product_id} className={styles.sumRow}>
                <span>
                  {line.title} × {line.quantity}
                  {line.sku && <span className={styles.muted}> ({line.sku})</span>}
                </span>
                <span>{formatMoney(line.line_total, order.currency)}</span>
              </div>
            ))}
            {order.adjustments.map((adjustment, index) => (
              <div key={index} className={styles.sumRow}>
                <span>
                  {adjustment.label}
                  {adjustment.source && (
                    <span className={styles.muted}> · {adjustment.source}</span>
                  )}
                </span>
                <span>{formatMoney(adjustment.amount, order.currency)}</span>
              </div>
            ))}
            {order.refunded_amount > 0 && (
              <div className={cx(styles.sumRow, styles.warn)}>
                <span>{t('orders.refundedAmount')}</span>
                <span>-{formatMoney(order.refunded_amount, order.currency)}</span>
              </div>
            )}
            <div className={cx(styles.sumRow, styles.sumTotal)}>
              <span>{t('orders.total')}</span>
              <span>{formatMoney(order.total, order.currency)}</span>
            </div>
          </div>

          {order.refunds && order.refunds.length > 0 && (
            <div className={styles.box} style={{ whiteSpace: 'normal' }}>
              <div className={styles.boxTitle}>{t('orders.refundHistory')}</div>
              {order.refunds.map((ref) => (
                <div key={ref.id} className={styles.sumRow}>
                  <span>
                    <span className={styles.mono}>{formatDate(ref.created_at)}</span>
                    {ref.reason && <span> · {ref.reason}</span>}
                    {ref.refund_ref && <span className={styles.muted}> ({ref.refund_ref})</span>}
                  </span>
                  <span className={cx(styles.numeric, styles.warn)}>
                    -{formatMoney(ref.amount, ref.currency || order.currency)}
                  </span>
                </div>
              ))}
            </div>
          )}

          <div className={styles.detailGrid}>
            <div className={styles.box}>
              <div className={styles.boxTitle}>{t('orders.payment')}</div>
              {order.payment_provider}
              {order.payment_ref && (
                <>
                  {'\n'}
                  <span className={styles.mono}>{order.payment_ref}</span>
                </>
              )}
              {order.transaction_id && (
                <>
                  {'\n'}
                  <span className={styles.muted}>Tx: </span>
                  <span className={styles.mono}>{order.transaction_id}</span>
                </>
              )}
              {order.coupon_code && `\n${t('orders.coupon', { code: order.coupon_code })}`}
            </div>
            <div className={styles.box}>
              <div className={styles.boxTitle}>{t('orders.note')}</div>
              {order.note || <span className={styles.muted}>—</span>}
            </div>
          </div>

          {(order.status !== 'completed' &&
            order.status !== 'cancelled' &&
            order.status !== 'refunded') ||
          order.can_refund ? (
            <>
              {order.status !== 'completed' &&
                order.status !== 'cancelled' &&
                order.status !== 'refunded' && (
                  <Field label={t('orders.noteField')}>
                    <input
                      value={note}
                      onChange={(e) => setNote(e.target.value)}
                      placeholder={t('orders.notePlaceholder')}
                      aria-label={t('orders.noteAria')}
                    />
                  </Field>
                )}
              <div className={styles.actionsBar}>
                {order.status === 'pending_payment' && order.can_mark_paid && (
                  <Button variant="primary" disabled={busy} onClick={() => move('paid')}>
                    {t('orders.markPaid')}
                  </Button>
                )}
                {order.status === 'pending_payment' && !order.can_mark_paid && (
                  <span className={styles.muted}>{t('orders.waitingProvider')}</span>
                )}
                {order.status === 'paid' && (
                  <Button variant="primary" disabled={busy} onClick={() => move('shipped')}>
                    {t('orders.markShipped')}
                  </Button>
                )}
                {order.status === 'shipped' && (
                  <Button variant="primary" disabled={busy} onClick={() => move('completed')}>
                    {t('orders.markCompleted')}
                  </Button>
                )}
                {(order.status === 'pending_payment' || order.status === 'paid') && (
                  <Button danger disabled={busy} onClick={() => move('cancelled')}>
                    {t('orders.cancel')}
                  </Button>
                )}
                {order.can_refund && (
                  <Button variant="tool" disabled={busy} onClick={() => setShowRefund(true)}>
                    <RotateCcw size={14} />
                    <span>{t('orders.refund')}</span>
                  </Button>
                )}
              </div>
            </>
          ) : null}
        </div>
      </Modal>

      {showRefund && (
        <RefundModal
          order={order}
          onRefunded={onUpdated}
          onError={onError}
          onSuccess={onSuccess}
          onClose={() => setShowRefund(false)}
        />
      )}
    </>
  );
};

interface RefundModalProps {
  order: Order;
  onRefunded: (order: Order) => void;
  onError: (message: string) => void;
  onSuccess: (message: string) => void;
  onClose: () => void;
}

const RefundModal: React.FC<RefundModalProps> = ({
  order,
  onRefunded,
  onError,
  onSuccess,
  onClose,
}) => {
  const { t } = useTranslation();
  const maxRefundable = Math.max(0, order.total - order.refunded_amount);
  const [amountStr, setAmountStr] = useState<string>('');
  const [reason, setReason] = useState<string>('');
  const [restock, setRestock] = useState<boolean>(true);
  const [busy, setBusy] = useState<boolean>(false);

  const previewAmount = amountStr.trim() === '' ? maxRefundable : Number(amountStr);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = amountStr.trim();
    const amount = trimmed === '' ? maxRefundable : Number(trimmed);

    if (Number.isNaN(amount) || amount <= 0 || amount > maxRefundable) {
      onError(
        t('orders.invalidRefundAmount', {
          amount: formatMoney(maxRefundable, order.currency),
        })
      );
      return;
    }

    setBusy(true);
    try {
      const updated = await aegisApi.refundOrder(order.id, {
        amount: trimmed === '' ? undefined : amount,
        reason: reason.trim() || undefined,
        restock,
      });
      onSuccess(t('orders.refundSuccess'));
      onRefunded(updated);
      onClose();
    } catch (err) {
      onError(messageOf(err, t('api.refundOrderFailed')));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('orders.refundTitle', { number: order.number })}
      icon={<RotateCcw size={18} />}
      onClose={onClose}
      flush
    >
      <form onSubmit={handleSubmit} className={styles.detail}>
        <div className={cx(styles.notice, !order.supports_online_refund && styles.warn)}>
          {order.supports_online_refund
            ? t('orders.onlineRefundNotice')
            : t('orders.manualRefundNotice')}
        </div>

        <div className={styles.sumRow}>
          <span className={styles.muted}>
            {t('orders.maxRefundable', {
              amount: formatMoney(maxRefundable, order.currency),
            })}
          </span>
          <Button type="button" variant="tool" onClick={() => setAmountStr(String(maxRefundable))}>
            {t('orders.fullRefund')}
          </Button>
        </div>

        <Field label={t('orders.refundAmount')}>
          <input
            type="number"
            min={1}
            max={maxRefundable}
            step={1}
            value={amountStr}
            onChange={(e) => setAmountStr(e.target.value)}
            placeholder={t('orders.refundAmountPlaceholder')}
            aria-label={t('orders.refundAmount')}
          />
        </Field>
        {!Number.isNaN(previewAmount) && previewAmount > 0 && (
          <span className={styles.hint}>{formatMoney(previewAmount, order.currency)}</span>
        )}

        <Field label={t('orders.refundReason')}>
          <input
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder={t('orders.refundReasonPlaceholder')}
            aria-label={t('orders.refundReason')}
          />
        </Field>

        <label
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            fontSize: '0.85rem',
            cursor: 'pointer',
          }}
        >
          <input type="checkbox" checked={restock} onChange={(e) => setRestock(e.target.checked)} />
          <span>{t('orders.refundRestock')}</span>
        </label>

        <div className={styles.actionsBar} style={{ justifyContent: 'flex-end', marginTop: 12 }}>
          <Button type="button" variant="tool" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" danger disabled={busy || maxRefundable <= 0}>
            <RotateCcw size={14} />
            <span>{t('orders.executeRefund')}</span>
          </Button>
        </div>
      </form>
    </Modal>
  );
};
