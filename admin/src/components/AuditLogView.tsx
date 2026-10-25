import React, { useEffect, useMemo, useState } from 'react';
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  RefreshCw,
  ScrollText,
  Search,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { aegisApi, ApiError } from '../api/client';
import i18n from '../i18n';
import type { AuditAction, AuditEvent, AuditLogPage, AuditLogQuery } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import { RoleLabel } from './ui/RoleLabel';
import styles from './AuditLogView.module.css';

const PAGE_SIZE = 50;

const OUTCOME_BADGE: Record<string, string | undefined> = {
  success: styles.success,
  failure: styles.failure,
  denied: styles.denied,
};

const ACTIONS: AuditAction[] = [
  'content.create',
  'content.update',
  'content.delete',
  'content.publish',
  'session.create',
  'session.delete',
  'storage.upload',
  'storage.delete',
  'plugin.error',
  'plugin.settings_update',
  'plugin.enable',
  'plugin.disable',
  'plugin.reorder',
  'plugin.reset',
  'audit.purge',
  'access.denied',
  'theme.switch',
  'site.update',
  'user.create',
  'user.update',
  'user.delete',
  'user.register',
  'order.create',
  'order.update',
  'notification.send',
  'comment.create',
  'comment.moderate',
  'comment.delete',
  'comment.settings_update',
];
const OUTCOMES = ['success', 'failure', 'denied'] as const;

const isOutcome = (value: string): value is (typeof OUTCOMES)[number] =>
  (OUTCOMES as readonly string[]).includes(value);

interface Filters {
  action: string;
  outcome: string;
  actor_id: string;
  resource_type: string;
  resource_id: string;
  since: string;
  until: string;
}

const EMPTY_FILTERS: Filters = {
  action: '',
  outcome: '',
  actor_id: '',
  resource_type: '',
  resource_id: '',
  since: '',
  until: '',
};

/** Converts a `datetime-local` value (local time, no zone) to an RFC 3339 UTC timestamp */
const toRfc3339 = (localValue: string): string | undefined =>
  localValue ? new Date(localValue).toISOString() : undefined;

const describeError = (err: unknown, t: TFunction): string => {
  if (err instanceof ApiError) {
    switch (err.status) {
      case 403:
        return t('audit.forbidden');
      case 404:
        return t('audit.unavailable');
      default:
        return err.message;
    }
  }
  return err instanceof Error ? err.message : t('audit.loadFailed');
};

export const AuditLogView: React.FC = () => {
  const { t } = useTranslation();
  const outcomeLabel = (outcome: string): string =>
    isOutcome(outcome) ? t(`audit.outcomes.${outcome}`) : outcome;
  const [draft, setDraft] = useState<Filters>(EMPTY_FILTERS);
  const [filters, setFilters] = useState<Filters>(EMPTY_FILTERS);
  const [offset, setOffset] = useState<number>(0);
  const [reloadCount, setReloadCount] = useState<number>(0);
  const [result, setResult] = useState<{
    key: string;
    page: AuditLogPage | null;
    error: string | null;
  } | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const query = useMemo<AuditLogQuery>(
    () => ({
      action: filters.action,
      outcome: filters.outcome,
      actor_id: filters.actor_id.trim(),
      resource_type: filters.resource_type.trim(),
      resource_id: filters.resource_id.trim(),
      since: toRfc3339(filters.since),
      until: toRfc3339(filters.until),
      limit: PAGE_SIZE,
      offset,
    }),
    [filters, offset]
  );
  // A request is identified by its query and reload counter; loading = no result for it yet
  const requestKey = JSON.stringify([query, reloadCount]);
  const isLoading = result?.key !== requestKey;
  const page = result?.key === requestKey ? result.page : (result?.page ?? null);
  const error = result?.key === requestKey ? result.error : null;

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getAuditLogs(query)
      .then((loaded) => ({ page: loaded, error: null }))
      .catch((err: unknown) => ({ page: null, error: describeError(err, i18n.t) }))
      .then((outcome) => {
        if (!cancelled) setResult({ key: requestKey, ...outcome });
      });
    return () => {
      cancelled = true;
    };
  }, [query, requestKey]);

  const reload = () => setReloadCount((n) => n + 1);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setFilters(draft);
    setOffset(0);
  };

  const handleReset = () => {
    setDraft(EMPTY_FILTERS);
    setFilters(EMPTY_FILTERS);
    setOffset(0);
  };

  const updateDraft =
    (key: keyof Filters) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setDraft({ ...draft, [key]: e.target.value });

  const items: AuditEvent[] = page?.items ?? [];
  const hasNext = items.length === PAGE_SIZE;

  return (
    <main className={styles.main}>
      <div className={styles.column}>
        <div className={styles.heading}>
          <div className={styles.title}>
            <ScrollText size={20} />
            <h1>{t('audit.title')}</h1>
          </div>
          {page && (
            <span className={styles.retention}>
              {t('audit.retention', {
                value: page.retention_days
                  ? t('audit.retentionDays', { count: page.retention_days })
                  : t('audit.unlimited'),
              })}
            </span>
          )}
        </div>

        <form className={styles.filters} onSubmit={handleSearch}>
          <Field label={t('audit.fields.action')}>
            <select value={draft.action} onChange={updateDraft('action')}>
              <option value="">{t('audit.all')}</option>
              {ACTIONS.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t('audit.fields.outcome')}>
            <select value={draft.outcome} onChange={updateDraft('outcome')}>
              <option value="">{t('audit.all')}</option>
              {OUTCOMES.map((o) => (
                <option key={o} value={o}>
                  {t(`audit.outcomes.${o}`)}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t('audit.fields.actor')}>
            <input
              value={draft.actor_id}
              onChange={updateDraft('actor_id')}
              placeholder={t('audit.placeholders.actor')}
            />
          </Field>
          <Field label={t('audit.fields.resource')}>
            <input
              value={draft.resource_type}
              onChange={updateDraft('resource_type')}
              placeholder={t('audit.placeholders.resourceType')}
            />
            <input
              value={draft.resource_id}
              onChange={updateDraft('resource_id')}
              placeholder={t('audit.placeholders.resourceId')}
            />
          </Field>
          <Field label={t('audit.fields.since')}>
            <input type="datetime-local" value={draft.since} onChange={updateDraft('since')} />
          </Field>
          <Field label={t('audit.fields.until')}>
            <input type="datetime-local" value={draft.until} onChange={updateDraft('until')} />
          </Field>
          <div className={styles.filterActions}>
            <Button type="submit" variant="primary">
              <Search size={16} />
              <span>{t('audit.search')}</span>
            </Button>
            <Button onClick={handleReset}>{t('audit.reset')}</Button>
            <Button variant="tool" onClick={reload} disabled={isLoading} title={t('common.reload')}>
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
              <span>{t('audit.refresh')}</span>
            </Button>
          </div>
        </form>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {!error && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('audit.columns.time')}</th>
                  <th>{t('audit.columns.action')}</th>
                  <th>{t('audit.columns.outcome')}</th>
                  <th>{t('audit.columns.actor')}</th>
                  <th>{t('audit.columns.resource')}</th>
                  <th>{t('audit.columns.ip')}</th>
                  <th aria-label={t('audit.columns.details')} />
                </tr>
              </thead>
              <tbody>
                {items.map((event) => {
                  const expanded = expandedId === event.id;
                  return (
                    <React.Fragment key={event.id}>
                      <tr
                        className={cx(styles.row, expanded && styles.expanded)}
                        onClick={() => setExpandedId(expanded ? null : event.id)}
                      >
                        <td className={styles.mono} title={event.occurred_at}>
                          {formatDateTime(event.occurred_at)}
                        </td>
                        <td className={styles.mono}>{event.action}</td>
                        <td>
                          <span className={cx(styles.outcome, OUTCOME_BADGE[event.outcome])}>
                            {outcomeLabel(event.outcome)}
                          </span>
                        </td>
                        <td>
                          {event.actor ? (
                            <>
                              {event.actor.user_id}
                              <span className={styles.muted}>
                                {' '}
                                (<RoleLabel role={event.actor.role} />)
                              </span>
                            </>
                          ) : (
                            <span className={styles.muted}>—</span>
                          )}
                        </td>
                        <td className={cx(styles.mono, styles.resource)}>
                          {event.resource_type ?? '—'}
                          {event.resource_id && (
                            <span className={styles.muted}> / {event.resource_id}</span>
                          )}
                        </td>
                        <td className={styles.mono}>{event.ip ?? '—'}</td>
                        <td>{expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</td>
                      </tr>
                      {expanded && (
                        <tr className={styles.detailRow}>
                          <td colSpan={7}>
                            <dl className={styles.detailMeta}>
                              <dt>{t('audit.id')}</dt>
                              <dd className={styles.mono}>{event.id}</dd>
                              <dt>{t('audit.userAgent')}</dt>
                              <dd>{event.user_agent ?? '—'}</dd>
                            </dl>
                            <pre className={styles.details}>
                              {JSON.stringify(event.details, null, 2)}
                            </pre>
                          </td>
                        </tr>
                      )}
                    </React.Fragment>
                  );
                })}
              </tbody>
            </table>

            {page && items.length === 0 && <div className={styles.empty}>{t('audit.empty')}</div>}

            <div className={styles.pagination}>
              <Button
                variant="tool"
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                disabled={offset === 0 || isLoading}
              >
                <ChevronLeft size={14} />
                <span>{t('audit.newer')}</span>
              </Button>
              <span className={styles.muted}>
                {items.length > 0 ? `${offset + 1}–${offset + items.length}` : t('audit.noEvents')}
              </span>
              <Button
                variant="tool"
                onClick={() => setOffset(offset + PAGE_SIZE)}
                disabled={!hasNext || isLoading}
              >
                <span>{t('audit.older')}</span>
                <ChevronRight size={14} />
              </Button>
            </div>
          </div>
        )}
      </div>
    </main>
  );
};
