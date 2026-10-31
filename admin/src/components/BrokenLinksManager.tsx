import React, { useEffect, useMemo, useState } from 'react';
import {
  Unlink,
  RefreshCw,
  Search,
  ExternalLink,
  CheckCircle2,
  AlertTriangle,
  Loader2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { BrokenLinksReport } from '../types';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import styles from './BrokenLinksManager.module.css';

interface BrokenLinksManagerProps {
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

type TypeFilter = 'all' | 'internal' | 'external';

export const BrokenLinksManager: React.FC<BrokenLinksManagerProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [report, setReport] = useState<BrokenLinksReport | null>(null);
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(true);
  const [query, setQuery] = useState<string>('');
  const [typeFilter, setTypeFilter] = useState<TypeFilter>('all');
  const [reloadCount, setReloadCount] = useState<number>(0);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getBrokenLinks()
      .then((data) => {
        if (cancelled) return;
        setReport(data.report);
        setIsScanning(data.is_scanning);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const msg = err instanceof Error ? err.message : t('brokenLinks.loadFailed');
        onShowToast(msg, 'error');
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [reloadCount, t, onShowToast]);

  // Poll while scan is active
  useEffect(() => {
    if (!isScanning) return;
    const timer = setInterval(() => {
      setReloadCount((c) => c + 1);
    }, 2500);
    return () => clearInterval(timer);
  }, [isScanning]);

  const handleStartScan = async () => {
    if (isScanning) return;
    try {
      setIsScanning(true);
      await aegisApi.triggerBrokenLinksScan();
      onShowToast(t('brokenLinks.scanStarted'), 'success');
      setReloadCount((c) => c + 1);
    } catch (err: unknown) {
      setIsScanning(false);
      const msg = err instanceof Error ? err.message : t('brokenLinks.scanFailed');
      onShowToast(msg, 'error');
    }
  };

  const filteredItems = useMemo(() => {
    if (!report?.items) return [];
    return report.items.filter((item) => {
      if (typeFilter === 'internal' && item.is_external) return false;
      if (typeFilter === 'external' && !item.is_external) return false;

      if (!query.trim()) return true;
      const q = query.toLowerCase();
      return (
        item.target_url.toLowerCase().includes(q) ||
        item.content_title.toLowerCase().includes(q) ||
        item.content_slug.toLowerCase().includes(q) ||
        item.content_type.toLowerCase().includes(q) ||
        (item.error_message && item.error_message.toLowerCase().includes(q)) ||
        (item.status_code && String(item.status_code).includes(q))
      );
    });
  }, [report, typeFilter, query]);

  const formattedDate = report?.last_scanned_at
    ? (() => {
        try {
          return new Date(report.last_scanned_at).toLocaleString();
        } catch {
          return report.last_scanned_at;
        }
      })()
    : t('brokenLinks.neverScanned');

  return (
    <div className={styles.main}>
      <div className={styles.column}>
        <div className={styles.headerRow}>
          <div className={styles.titleArea}>
            <div className={styles.title}>
              <Unlink size={24} />
              <h1>{t('brokenLinks.title')}</h1>
            </div>
            <p className={styles.description}>{t('brokenLinks.description')}</p>
          </div>
          <div>
            <Button
              variant="primary"
              className={styles.scanBtn}
              onClick={handleStartScan}
              disabled={isScanning || loading}
            >
              {isScanning ? (
                <>
                  <Loader2 size={16} className={styles.spin} />
                  <span>{t('brokenLinks.scanning')}</span>
                </>
              ) : (
                <>
                  <RefreshCw size={16} />
                  <span>{t('brokenLinks.scanNow')}</span>
                </>
              )}
            </Button>
          </div>
        </div>

        {/* Stats Row */}
        <div className={styles.statsRow}>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>{t('brokenLinks.lastScannedAt')}</span>
            <span
              className={cx(styles.statValue, !report?.last_scanned_at && styles.statValueMuted)}
            >
              {formattedDate}
            </span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>{t('brokenLinks.totalChecked')}</span>
            <span className={styles.statValue}>{report?.total_links_checked ?? 0}</span>
          </div>
          <div className={styles.statCard}>
            <span className={styles.statLabel}>{t('brokenLinks.brokenCount')}</span>
            <span
              className={cx(
                styles.statValue,
                (report?.broken_count ?? 0) > 0
                  ? styles.statValueDanger
                  : report?.last_scanned_at
                    ? styles.statValueSuccess
                    : undefined
              )}
            >
              {report?.broken_count ?? 0}
            </span>
          </div>
        </div>

        {/* Filter and Search Bar */}
        <div className={styles.filterBar}>
          <div className={styles.searchWrapper}>
            <Search size={16} className={styles.searchIcon} />
            <input
              type="text"
              className={styles.searchInput}
              placeholder={t('brokenLinks.filterPlaceholder')}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <div className={styles.typeFilter}>
            <button
              type="button"
              className={cx(styles.typeBtn, typeFilter === 'all' && styles.typeBtnActive)}
              onClick={() => setTypeFilter('all')}
            >
              All
            </button>
            <button
              type="button"
              className={cx(styles.typeBtn, typeFilter === 'internal' && styles.typeBtnActive)}
              onClick={() => setTypeFilter('internal')}
            >
              {t('brokenLinks.internal')}
            </button>
            <button
              type="button"
              className={cx(styles.typeBtn, typeFilter === 'external' && styles.typeBtnActive)}
              onClick={() => setTypeFilter('external')}
            >
              {t('brokenLinks.external')}
            </button>
          </div>
        </div>

        {/* Table or Empty States */}
        {loading ? (
          <div className={styles.emptyState}>
            <Loader2 size={32} className={styles.spin} />
          </div>
        ) : !report?.last_scanned_at ? (
          <div className={styles.emptyState}>
            <AlertTriangle size={40} className={styles.emptyIconNeutral} />
            <div className={styles.emptyTitle}>{t('brokenLinks.title')}</div>
            <div className={styles.emptyDesc}>{t('brokenLinks.emptyState')}</div>
          </div>
        ) : report.broken_count === 0 ? (
          <div className={styles.emptyState}>
            <CheckCircle2 size={40} className={styles.emptyIconSuccess} />
            <div className={styles.emptyTitle}>{t('brokenLinks.noBrokenLinks')}</div>
            <div className={styles.emptyDesc}>
              {t('brokenLinks.totalChecked')}: {report.total_links_checked}
            </div>
          </div>
        ) : (
          <div className={styles.tableWrapper}>
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>{t('brokenLinks.targetUrl')}</th>
                  <th>{t('brokenLinks.sourceContent')}</th>
                  <th>{t('brokenLinks.type')}</th>
                  <th>{t('brokenLinks.statusOrError')}</th>
                </tr>
              </thead>
              <tbody>
                {filteredItems.map((item) => (
                  <tr key={item.id}>
                    <td>
                      <div className={styles.targetUrl}>
                        {item.is_external ? (
                          <a href={item.target_url} target="_blank" rel="noopener noreferrer">
                            {item.target_url}
                          </a>
                        ) : (
                          <span>{item.target_url}</span>
                        )}
                        {item.is_external && <ExternalLink size={12} />}
                      </div>
                    </td>
                    <td>
                      <div className={styles.contentTitle}>{item.content_title}</div>
                      <div className={styles.contentMeta}>
                        <span className={styles.contentBadge}>
                          {item.content_type}: {item.content_slug}
                        </span>
                      </div>
                    </td>
                    <td>
                      <span className={styles.typeBadge}>
                        {item.is_external ? t('brokenLinks.external') : t('brokenLinks.internal')}
                      </span>
                    </td>
                    <td>
                      <div>
                        {item.status_code ? (
                          <span
                            className={cx(
                              styles.statusBadge,
                              item.status_code >= 400 ? styles.statusError : styles.statusWarning
                            )}
                          >
                            HTTP {item.status_code}
                          </span>
                        ) : null}
                        {item.error_message && (
                          <div className={styles.reasonText}>{item.error_message}</div>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
