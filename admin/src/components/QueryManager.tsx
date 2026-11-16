import React, { useEffect, useMemo, useState } from 'react';
import { Database, RefreshCw, Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { QueriesResponse, QueryItem } from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import styles from './QueryManager.module.css';

interface QueryManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

type ModeFilter = 'all' | 'aggregate' | 'list';
type PreviewMode = 'table' | 'json';

function isRelationValue(val: unknown): val is { id: string; title: string; slug: string } {
  return (
    typeof val === 'object' &&
    val !== null &&
    !Array.isArray(val) &&
    'id' in val &&
    'title' in val &&
    'slug' in val
  );
}

function formatCellValue(val: unknown): string {
  if (val === null || val === undefined) {
    return '-';
  }
  if (isRelationValue(val)) {
    return `${val.title} (${val.slug})`;
  }
  if (typeof val === 'object') {
    return JSON.stringify(val);
  }
  return String(val);
}

function tableColumns(item: QueryItem): string[] {
  if (item.mode === 'list') {
    return ['title', 'slug', 'published_at', 'fields'];
  }
  if (item.rows.length === 0) {
    return [];
  }
  return Object.keys(item.rows[0]);
}

function formatClauses(item: QueryItem): string[] {
  const parts: string[] = [];
  const whereKeys = Object.keys(item.where);
  if (whereKeys.length > 0) {
    parts.push(`where: ${whereKeys.join(', ')}`);
  }
  if (item.group_by.length > 0) {
    const labels = item.group_by.map((g) => {
      const base = g.bucket ? `${g.field}:${g.bucket}` : g.field;
      return g.as ? `${base} -> ${g.as}` : base;
    });
    parts.push(`group_by: ${labels.join(', ')}`);
  }
  if (item.select.length > 0) {
    const labels = item.select.map((s) =>
      s.field ? `${s.fn}(${s.field}) as ${s.as}` : `${s.fn}() as ${s.as}`
    );
    parts.push(`select: ${labels.join(', ')}`);
  }
  if (item.order_by.length > 0) {
    parts.push(`order_by: ${item.order_by.map((o) => `${o.field} ${o.dir}`).join(', ')}`);
  }
  if (item.limit !== null && item.limit !== undefined) {
    parts.push(`limit: ${item.limit}`);
  }
  return parts;
}

export const QueryManager: React.FC<QueryManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [data, setData] = useState<QueriesResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [modeFilter, setModeFilter] = useState<ModeFilter>('all');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [viewModes, setViewModes] = useState<Record<string, PreviewMode>>({});
  const [reloadCount, setReloadCount] = useState<number>(0);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getQueries()
      .then((res) => {
        if (cancelled) return;
        setData(res);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const msg = err instanceof Error ? err.message : t('queries.loadFailed');
        onShowToast(msg, 'error');
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount, t, onShowToast]);

  const queries = data?.queries ?? [];

  const filteredQueries = useMemo(() => {
    const items = data?.queries ?? [];
    const q = searchQuery.trim().toLowerCase();
    return items.filter((item) => {
      if (modeFilter !== 'all' && item.mode !== modeFilter) {
        return false;
      }
      if (!q) {
        return true;
      }
      return item.name.toLowerCase().includes(q) || item.from.toLowerCase().includes(q);
    });
  }, [data, modeFilter, searchQuery]);

  const getPreviewMode = (name: string): PreviewMode => viewModes[name] ?? 'table';

  const setPreviewMode = (name: string, mode: PreviewMode) => {
    setViewModes((prev) => ({ ...prev, [name]: mode }));
  };

  return (
    <main className={styles.main} role="region" aria-label={t('queries.title')}>
      <div className={styles.column}>
        <div className={styles.headerRow}>
          <div className={styles.titleArea}>
            <div className={styles.title}>
              <Database size={24} />
              <h1>{t('queries.title')}</h1>
            </div>
            <p className={styles.description}>{t('queries.description')}</p>
          </div>
          <div className={styles.headerActions}>
            {data && (
              <span className={styles.tzBadge}>{t('queries.timezone', { tz: data.timezone })}</span>
            )}
            <Button
              variant="secondary"
              className={styles.refreshBtn}
              onClick={() => {
                setLoading(true);
                setReloadCount((c) => c + 1);
              }}
              disabled={loading}
            >
              <RefreshCw size={15} />
              <span>{t('queries.refresh')}</span>
            </Button>
          </div>
        </div>

        {queries.length > 0 && (
          <div className={styles.toolbar}>
            <div className={styles.tabs} role="tablist" aria-label={t('queries.modeFilterAria')}>
              <button
                type="button"
                role="tab"
                aria-selected={modeFilter === 'all'}
                className={cx(styles.tab, modeFilter === 'all' && styles.tabActive)}
                onClick={() => setModeFilter('all')}
              >
                {t('queries.filterAll')}
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={modeFilter === 'aggregate'}
                className={cx(styles.tab, modeFilter === 'aggregate' && styles.tabActive)}
                onClick={() => setModeFilter('aggregate')}
              >
                {t('queries.modeAggregate')}
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={modeFilter === 'list'}
                className={cx(styles.tab, modeFilter === 'list' && styles.tabActive)}
                onClick={() => setModeFilter('list')}
              >
                {t('queries.modeList')}
              </button>
            </div>

            <div className={styles.searchWrapper}>
              <Search size={16} className={styles.searchIcon} />
              <input
                type="search"
                className={styles.searchInput}
                placeholder={t('queries.searchPlaceholder')}
                aria-label={t('queries.searchPlaceholder')}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
            </div>
          </div>
        )}

        {!loading && queries.length === 0 ? (
          <div className={styles.emptyState}>
            <div className={styles.emptyTitle}>{t('queries.emptyTitle')}</div>
            <div className={styles.emptyDesc}>{t('queries.emptyHint')}</div>
          </div>
        ) : !loading && filteredQueries.length === 0 ? (
          <div className={styles.emptyState}>
            <div className={styles.emptyTitle}>{t('queries.noResults')}</div>
          </div>
        ) : (
          <div className={styles.list}>
            {filteredQueries.map((item) => {
              const previewMode = getPreviewMode(item.name);
              const clauses = formatClauses(item);
              const columns = tableColumns(item);
              return (
                <section key={item.name} className={styles.card}>
                  <div className={styles.cardHeader}>
                    <div className={styles.cardTitleGroup}>
                      <span className={styles.queryName}>{`queries.${item.name}`}</span>
                      <span className={cx(styles.badge, styles.badgeAccent)}>
                        {item.mode === 'aggregate'
                          ? t('queries.modeAggregate')
                          : t('queries.modeList')}
                      </span>
                      <span className={cx(styles.badge, styles.badgeMono)}>
                        {`from: ${item.from}`}
                      </span>
                      <span className={styles.badge}>
                        {t('queries.rowCount', { count: item.rows.length })}
                      </span>
                    </div>

                    <div className={styles.viewToggle}>
                      <button
                        type="button"
                        className={cx(
                          styles.viewBtn,
                          previewMode === 'table' && styles.viewBtnActive
                        )}
                        aria-pressed={previewMode === 'table'}
                        onClick={() => setPreviewMode(item.name, 'table')}
                      >
                        {t('queries.viewTable')}
                      </button>
                      <button
                        type="button"
                        className={cx(
                          styles.viewBtn,
                          previewMode === 'json' && styles.viewBtnActive
                        )}
                        aria-pressed={previewMode === 'json'}
                        onClick={() => setPreviewMode(item.name, 'json')}
                      >
                        {t('queries.viewJson')}
                      </button>
                    </div>
                  </div>

                  {clauses.length > 0 && (
                    <div className={styles.clauses}>
                      {clauses.map((clause) => (
                        <span key={clause} className={styles.clauseChip}>
                          {clause}
                        </span>
                      ))}
                    </div>
                  )}

                  {item.rows.length === 0 ? (
                    <div className={styles.noRows}>{t('queries.noRows')}</div>
                  ) : previewMode === 'json' ? (
                    <pre className={styles.jsonPre}>
                      <code>{JSON.stringify(item.rows, null, 2)}</code>
                    </pre>
                  ) : (
                    <div className={styles.tableWrapper}>
                      <table className={styles.table}>
                        <thead>
                          <tr>
                            {columns.map((col) => (
                              <th key={col}>{col}</th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {item.rows.map((row, idx) => (
                            <tr key={idx}>
                              {columns.map((col) => (
                                <td key={col}>{formatCellValue(row[col])}</td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </section>
              );
            })}
          </div>
        )}
      </div>
    </main>
  );
};
