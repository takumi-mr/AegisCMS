import React, { useEffect, useState } from 'react';
import { ChevronLeft, ChevronRight, RefreshCw, ShoppingBag } from 'lucide-react';
import { aegisApi, ApiError } from '../api/client';
import type { Order, OrderPage, OrderStatus } from '../types';
import { cx } from '../utils/cx';
import { formatMoney } from '../utils/money';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { Modal } from './ui/Modal';
import styles from './Shop.module.css';

const PAGE_SIZE = 25;

const STATUS_LABELS: Record<OrderStatus, string> = {
  pending_payment: 'Awaiting payment',
  paid: 'Paid',
  shipped: 'Shipped',
  completed: 'Completed',
  cancelled: 'Cancelled',
};

const STATUS_BADGE: Record<OrderStatus, string | undefined> = {
  pending_payment: styles.warn,
  paid: styles.good,
  shipped: styles.good,
  completed: undefined,
  cancelled: styles.bad,
};

const FILTERS: (OrderStatus | '')[] = [
  '',
  'pending_payment',
  'paid',
  'shipped',
  'completed',
  'cancelled',
];

const formatDate = (iso: string | null): string => {
  if (!iso) return '—';
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
};

const messageOf = (err: unknown, fallback: string): string =>
  err instanceof Error ? err.message : fallback;

interface OrderManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Shop orders for editors: filter by status, open an order and move it along */
export const OrderManager: React.FC<OrderManagerProps> = ({ onShowToast }) => {
  const [status, setStatus] = useState<OrderStatus | ''>('');
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [page, setPage] = useState<OrderPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Order | null>(null);

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
            ? 'The shop is not enabled. Set [commerce].enabled = true in the server configuration.'
            : messageOf(err, 'Failed to load orders')
        );
      });
    return () => {
      cancelled = true;
    };
  }, [status, offset, reloadCount]);

  const handleUpdated = (updated: Order) => {
    setSelected(updated);
    setPage((prev) =>
      prev ? { ...prev, orders: prev.orders.map((o) => (o.id === updated.id ? updated : o)) } : prev
    );
    onShowToast(`Order ${updated.number}: ${STATUS_LABELS[updated.status]}`);
  };

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <ShoppingBag size={22} />
            <h1>Orders</h1>
            {page && <span className={styles.count}>{page.total}</span>}
          </div>
          <div className={styles.headingActions}>
            <Button variant="tool" onClick={() => setReloadCount((n) => n + 1)}>
              <RefreshCw size={14} />
              <span>Reload</span>
            </Button>
          </div>
        </div>

        <div className={styles.filters} role="tablist" aria-label="Order status">
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
              {value ? STATUS_LABELS[value] : 'All'}
            </button>
          ))}
        </div>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {page && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Order</th>
                  <th>Placed</th>
                  <th>Customer</th>
                  <th>Items</th>
                  <th className={styles.numeric}>Total</th>
                  <th>Status</th>
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
                        {STATUS_LABELS[order.status]}
                      </span>
                    </td>
                  </tr>
                ))}
                {page.orders.length === 0 && (
                  <tr>
                    <td colSpan={6} className={styles.empty}>
                      No orders{status ? ` with status "${STATUS_LABELS[status]}"` : ' yet'}.
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
              <span>Newer</span>
            </Button>
            <span>
              {offset + 1}–{Math.min(offset + PAGE_SIZE, page.total)} of {page.total}
            </span>
            <Button
              variant="tool"
              disabled={offset + PAGE_SIZE >= page.total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
            >
              <span>Older</span>
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
  onClose: () => void;
}

const OrderDetail: React.FC<OrderDetailProps> = ({ order, onUpdated, onError, onClose }) => {
  const [note, setNote] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);

  const move = async (status: OrderStatus) => {
    if (status === 'cancelled') {
      const refund =
        order.status === 'paid'
          ? ' The payment is not refunded automatically: refund it from your payment dashboard.'
          : '';
      if (!window.confirm(`Cancel order ${order.number}? Its stock is returned.${refund}`)) return;
    }
    setBusy(true);
    try {
      onUpdated(await aegisApi.updateOrderStatus(order.id, status, note.trim() || undefined));
      setNote('');
    } catch (err) {
      onError(messageOf(err, 'Failed to update the order'));
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
    <Modal title={`Order ${order.number}`} icon={<ShoppingBag size={18} />} onClose={onClose} wide>
      <div className={styles.detail}>
        <div className={styles.actionsBar}>
          <span className={cx(styles.badge, STATUS_BADGE[order.status])}>
            {STATUS_LABELS[order.status]}
          </span>
          <span className={styles.muted}>Placed {formatDate(order.created_at)}</span>
          {order.paid_at && (
            <span className={styles.muted}>· Paid {formatDate(order.paid_at)}</span>
          )}
          {order.status === 'pending_payment' && order.expires_at && (
            <span className={styles.muted}>· Stock held until {formatDate(order.expires_at)}</span>
          )}
        </div>

        <div className={styles.detailGrid}>
          <div className={styles.box}>
            <div className={styles.boxTitle}>Customer</div>
            {order.customer_name || '—'}
            {'\n'}
            <a href={`mailto:${order.customer_email}`}>{order.customer_email}</a>
          </div>
          <div className={styles.box}>
            <div className={styles.boxTitle}>Ship to</div>
            {addressText || <span className={styles.muted}>No address</span>}
          </div>
        </div>

        <div className={styles.box} style={{ whiteSpace: 'normal' }}>
          <div className={styles.boxTitle}>Items</div>
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
                {adjustment.source && <span className={styles.muted}> · {adjustment.source}</span>}
              </span>
              <span>{formatMoney(adjustment.amount, order.currency)}</span>
            </div>
          ))}
          <div className={cx(styles.sumRow, styles.sumTotal)}>
            <span>Total</span>
            <span>{formatMoney(order.total, order.currency)}</span>
          </div>
        </div>

        <div className={styles.detailGrid}>
          <div className={styles.box}>
            <div className={styles.boxTitle}>Payment</div>
            {order.payment_provider}
            {order.payment_ref && (
              <>
                {'\n'}
                <span className={styles.mono}>{order.payment_ref}</span>
              </>
            )}
            {order.coupon_code && `\nCoupon: ${order.coupon_code}`}
          </div>
          <div className={styles.box}>
            <div className={styles.boxTitle}>Note</div>
            {order.note || <span className={styles.muted}>—</span>}
          </div>
        </div>

        {order.status !== 'completed' && order.status !== 'cancelled' && (
          <>
            <Field label="note:">
              <input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="Optional, e.g. a tracking number"
                aria-label="Note"
              />
            </Field>
            <div className={styles.actionsBar}>
              {order.status === 'pending_payment' && order.can_mark_paid && (
                <Button variant="primary" disabled={busy} onClick={() => move('paid')}>
                  Mark as paid
                </Button>
              )}
              {order.status === 'pending_payment' && !order.can_mark_paid && (
                <span className={styles.muted}>
                  Waiting for the payment provider to confirm the payment.
                </span>
              )}
              {order.status === 'paid' && (
                <Button variant="primary" disabled={busy} onClick={() => move('shipped')}>
                  Mark as shipped
                </Button>
              )}
              {order.status === 'shipped' && (
                <Button variant="primary" disabled={busy} onClick={() => move('completed')}>
                  Mark as completed
                </Button>
              )}
              {(order.status === 'pending_payment' || order.status === 'paid') && (
                <Button danger disabled={busy} onClick={() => move('cancelled')}>
                  Cancel order
                </Button>
              )}
            </div>
          </>
        )}
      </div>
    </Modal>
  );
};
