import React, { useEffect, useMemo, useState } from 'react';
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  KeyRound,
  RefreshCw,
  ScrollText,
  Search,
} from 'lucide-react';
import { aegisApi, ApiError } from '../api/client';
import type { AuditEvent, AuditLogPage, AuditLogQuery } from '../types';

const PAGE_SIZE = 50;
const TOKEN_STORAGE_KEY = 'aegis.auditSessionToken';

const ACTIONS = [
  'content.create',
  'session.create',
  'session.delete',
  'storage.upload',
  'plugin.error',
  'audit.purge',
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

// sessionStorage can be unavailable (private mode, blocked storage); the token is a convenience only
const readStoredToken = (): string => {
  try {
    return sessionStorage.getItem(TOKEN_STORAGE_KEY) ?? '';
  } catch {
    return '';
  }
};

const storeToken = (token: string) => {
  try {
    if (token) sessionStorage.setItem(TOKEN_STORAGE_KEY, token);
    else sessionStorage.removeItem(TOKEN_STORAGE_KEY);
  } catch {
    // ignore
  }
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
      case 401:
        return 'Session token is missing, invalid or expired.';
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
  const [tokenInput, setTokenInput] = useState<string>(readStoredToken);
  const [token, setToken] = useState<string>(readStoredToken);
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
  // A request is identified by its token, query and reload counter; loading = no result for it yet
  const requestKey = JSON.stringify([token, query, reloadCount]);
  const isLoading = token !== '' && result?.key !== requestKey;
  const page = result?.key === requestKey ? result.page : (result?.page ?? null);
  const error = result?.key === requestKey ? result.error : null;

  useEffect(() => {
    if (!token) return;
    let cancelled = false;
    aegisApi
      .getAuditLogs(token, query)
      .then((loaded) => ({ page: loaded, error: null }))
      .catch((err: unknown) => ({ page: null, error: describeError(err) }))
      .then((outcome) => {
        if (!cancelled) setResult({ key: requestKey, ...outcome });
      });
    return () => {
      cancelled = true;
    };
  }, [token, query, requestKey]);

  const reload = () => setReloadCount((n) => n + 1);

  const handleConnect = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = tokenInput.trim();
    storeToken(trimmed);
    setToken(trimmed);
    setOffset(0);
  };

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

  const updateDraft = (key: keyof Filters) => (
    e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>
  ) => setDraft({ ...draft, [key]: e.target.value });

  const items: AuditEvent[] = page?.items ?? [];
  const hasNext = items.length === PAGE_SIZE;

  return (
    <main className="audit-main">
      <div className="audit-column">
        <div className="audit-heading">
          <div className="audit-title">
            <ScrollText size={20} />
            <h1>Audit Logs</h1>
          </div>
          {page && (
            <span className="audit-retention">
              Retention:{' '}
              {page.retention_days ? `${page.retention_days} days` : 'unlimited'}
            </span>
          )}
        </div>

        <form className="audit-token-row" onSubmit={handleConnect}>
          <div className="input-field">
            <KeyRound size={14} className="input-prefix" />
            <input
              type="password"
              placeholder="Admin session token (ses_...)"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              autoComplete="off"
              aria-label="Admin session token"
            />
          </div>
          <button type="submit" className="btn btn-secondary">
            Connect
          </button>
        </form>

        <form className="audit-filters" onSubmit={handleSearch}>
          <div className="input-field">
            <span className="input-prefix">action:</span>
            <select value={draft.action} onChange={updateDraft('action')}>
              <option value="">all</option>
              {ACTIONS.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </div>
          <div className="input-field">
            <span className="input-prefix">outcome:</span>
            <select value={draft.outcome} onChange={updateDraft('outcome')}>
              <option value="">all</option>
              {OUTCOMES.map((o) => (
                <option key={o} value={o}>
                  {o}
                </option>
              ))}
            </select>
          </div>
          <div className="input-field">
            <span className="input-prefix">actor:</span>
            <input value={draft.actor_id} onChange={updateDraft('actor_id')} placeholder="user id" />
          </div>
          <div className="input-field">
            <span className="input-prefix">resource:</span>
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
          </div>
          <div className="input-field">
            <span className="input-prefix">since:</span>
            <input type="datetime-local" value={draft.since} onChange={updateDraft('since')} />
          </div>
          <div className="input-field">
            <span className="input-prefix">until:</span>
            <input type="datetime-local" value={draft.until} onChange={updateDraft('until')} />
          </div>
          <div className="audit-filter-actions">
            <button type="submit" className="btn btn-primary" disabled={!token}>
              <Search size={16} />
              <span>Search</span>
            </button>
            <button type="button" className="btn btn-secondary" onClick={handleReset}>
              Reset
            </button>
            <button
              type="button"
              className="tool-btn"
              onClick={reload}
              disabled={!token || isLoading}
              title="Reload"
            >
              <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
              <span>Refresh</span>
            </button>
          </div>
        </form>

        {!token && (
          <div className="audit-notice">
            Enter an admin session token to read audit logs. Reading requires a session whose
            role is <code>admin</code>.
          </div>
        )}
        {error && <div className="audit-notice error">{error}</div>}

        {token && !error && (
          <div className="audit-table-wrap">
            <table className="audit-table">
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
                        className={`audit-row ${expanded ? 'expanded' : ''}`}
                        onClick={() => setExpandedId(expanded ? null : event.id)}
                      >
                        <td className="mono" title={event.occurred_at}>
                          {formatTimestamp(event.occurred_at)}
                        </td>
                        <td className="mono">{event.action}</td>
                        <td>
                          <span className={`badge-outcome ${event.outcome}`}>{event.outcome}</span>
                        </td>
                        <td>
                          {event.actor ? (
                            <>
                              {event.actor.user_id}
                              <span className="audit-muted"> ({event.actor.role})</span>
                            </>
                          ) : (
                            <span className="audit-muted">—</span>
                          )}
                        </td>
                        <td className="mono audit-resource">
                          {event.resource_type ?? '—'}
                          {event.resource_id && (
                            <span className="audit-muted"> / {event.resource_id}</span>
                          )}
                        </td>
                        <td className="mono">{event.ip ?? '—'}</td>
                        <td>{expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}</td>
                      </tr>
                      {expanded && (
                        <tr className="audit-detail-row">
                          <td colSpan={7}>
                            <dl className="audit-detail-meta">
                              <dt>ID</dt>
                              <dd className="mono">{event.id}</dd>
                              <dt>User agent</dt>
                              <dd>{event.user_agent ?? '—'}</dd>
                            </dl>
                            <pre className="audit-details">
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
              <div className="audit-empty">No audit events match these filters.</div>
            )}

            <div className="audit-pagination">
              <button
                className="tool-btn"
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                disabled={offset === 0 || isLoading}
              >
                <ChevronLeft size={14} />
                <span>Newer</span>
              </button>
              <span className="audit-muted">
                {items.length > 0 ? `${offset + 1}–${offset + items.length}` : '0 events'}
              </span>
              <button
                className="tool-btn"
                onClick={() => setOffset(offset + PAGE_SIZE)}
                disabled={!hasNext || isLoading}
              >
                <span>Older</span>
                <ChevronRight size={14} />
              </button>
            </div>
          </div>
        )}
      </div>
    </main>
  );
};
