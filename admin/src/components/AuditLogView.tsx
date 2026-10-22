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
import { aegisApi, ApiError } from '../api/client';
import type { AuditAction, AuditEvent, AuditLogPage, AuditLogQuery } from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import { Field } from './ui/Field';
import styles from './AuditLogView.module.css';

const PAGE_SIZE = 50;

const OUTCOME_BADGE: Record<string, string | undefined> = {
  success: styles.success,
  failure: styles.failure,
  denied: styles.denied,
};

const ACTIONS: AuditAction[] = [
  'content.create',
  'session.create',
  'session.delete',
  'storage.upload',
  'storage.delete',
  'plugin.error',
  'audit.purge',
  'access.denied',
];
const OUTCOMES = ['success', 'failure', 'denied'];

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

const formatTimestamp = (iso: string): string => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
};

const describeError = (err: unknown): string => {
  if (err instanceof ApiError) {
    switch (err.status) {
      case 403:
        return 'This session does not have the "admin" role required to read audit logs.';
      case 404:
        return 'Audit log query is unavailable: enable the "database" audit sink on the server.';
      default:
        return err.message;
    }
  }
  return err instanceof Error ? err.message : 'Failed to load audit logs';
};

export const AuditLogView: React.FC = () => {
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
      .catch((err: unknown) => ({ page: null, error: describeError(err) }))
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
            <h1>Audit Logs</h1>
          </div>
          {page && (
            <span className={styles.retention}>
              Retention: {page.retention_days ? `${page.retention_days} days` : 'unlimited'}
            </span>
          )}
        </div>

        <form className={styles.filters} onSubmit={handleSearch}>
          <Field label="action:">
            <select value={draft.action} onChange={updateDraft('action')}>
              <option value="">all</option>
              {ACTIONS.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </Field>
          <Field label="outcome:">
            <select value={draft.outcome} onChange={updateDraft('outcome')}>
              <option value="">all</option>
              {OUTCOMES.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          </Field>
          <Field label="actor:">
            <input
              value={draft.actor_id}
              onChange={updateDraft('actor_id')}
              placeholder="user id"
            />
          </Field>
          <Field label="resource:">
            <input
              value={draft.resource_type}
              onChange={updateDraft('resource_type')}
              placeholder="type"
            />
            <input
              value={draft.resource_id}
              onChange={updateDraft('resource_id')}
              placeholder="id"
            />
          </Field>
          <Field label="since:">
            <input type="datetime-local" value={draft.since} onChange={updateDraft('since')} />
          </Field>
          <Field label="until:">
            <input type="datetime-local" value={draft.until} onChange={updateDraft('until')} />
          </Field>
          <div className={styles.filterActions}>
            <Button type="submit" variant="primary">
              <Search size={16} />
              <span>Search</span>
            </Button>
            <Button onClick={handleReset}>Reset</Button>
            <Button variant="tool" onClick={reload} disabled={isLoading} title="Reload">
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
              <span>Refresh</span>
            </Button>
          </div>
        </form>

        {error && <div className={cx(styles.notice, styles.error)}>{error}</div>}

        {!error && (
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Action</th>
                  <th>Outcome</th>
                  <th>Actor</th>
                  <th>Resource</th>
                  <th>IP</th>
                  <th aria-label="Details" />
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
                          {formatTimestamp(event.occurred_at)}
                        </td>
                        <td className={styles.mono}>{event.action}</td>
                        <td>
                          <span className={cx(styles.outcome, OUTCOME_BADGE[event.outcome])}>
                            {event.outcome}
                          </span>
                        </td>
                        <td>
                          {event.actor ? (
                            <>
                              {event.actor.user_id}
                              <span className={styles.muted}> ({event.actor.role})</span>
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
                              <dt>ID</dt>
                              <dd className={styles.mono}>{event.id}</dd>
                              <dt>User agent</dt>
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

            {page && items.length === 0 && (
              <div className={styles.empty}>No audit events match these filters.</div>
            )}

            <div className={styles.pagination}>
              <Button
                variant="tool"
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                disabled={offset === 0 || isLoading}
              >
                <ChevronLeft size={14} />
                <span>Newer</span>
              </Button>
              <span className={styles.muted}>
                {items.length > 0 ? `${offset + 1}–${offset + items.length}` : '0 events'}
              </span>
              <Button
                variant="tool"
                onClick={() => setOffset(offset + PAGE_SIZE)}
                disabled={!hasNext || isLoading}
              >
                <span>Older</span>
                <ChevronRight size={14} />
              </Button>
            </div>
          </div>
        )}
      </div>
    </main>
  );
};
