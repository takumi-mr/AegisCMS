import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { BarChart3, Download, Minus, RefreshCw, TrendingDown, TrendingUp } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { AnalyticsDay, AnalyticsSummary, AnalyticsTotal } from '../types';
import { currentLanguage } from '../i18n';
import { labelIndexes, lastDays, lineChartGeometry } from '../utils/analytics';
import { cx } from '../utils/cx';
import { Button } from './ui/Button';
import styles from './AnalyticsView.module.css';

/** Ranges offered above the chart (days, ending today) */
const RANGES = [7, 30, 90] as const;
type RangeDays = (typeof RANGES)[number];

const CHART_HEIGHT = 240;
const PAD = { top: 16, right: 84, bottom: 28, left: 44 };
const TOOLTIP_WIDTH = 180;

interface AnalyticsViewProps {
  onShowToast?: (message: string, type?: 'success' | 'error') => void;
}

const formatNumber = (value: number): string => value.toLocaleString(currentLanguage());

/** `YYYY-MM-DD` as a short date in the Admin Studio language (the day is UTC, shown as is) */
const formatDay = (day: string, long = false): string => {
  const date = new Date(`${day}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return day;
  return date.toLocaleDateString(currentLanguage(), {
    timeZone: 'UTC',
    month: 'short',
    day: 'numeric',
    ...(long ? { year: 'numeric', weekday: 'short' } : {}),
  });
};

/** Daily pageviews and visitors as two lines on one axis, with a crosshair tooltip */
const DailyChart: React.FC<{ daily: AnalyticsDay[] }> = ({ daily }) => {
  const { t } = useTranslation();
  const wrapRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(720);
  const [active, setActive] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const update = () => setWidth(Math.max(280, Math.round(el.getBoundingClientRect().width)));
    update();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const geometry = lineChartGeometry(daily, width, CHART_HEIGHT, PAD);
  const last = geometry.points[geometry.points.length - 1];
  const lastDay = daily[daily.length - 1];
  // Direct labels at the line ends, kept apart when the two values are close
  let pageviewsLabelY = last?.pageviewsY ?? 0;
  let visitorsLabelY = last?.visitorsY ?? 0;
  if (Math.abs(visitorsLabelY - pageviewsLabelY) < 14) {
    const mid = (pageviewsLabelY + visitorsLabelY) / 2;
    pageviewsLabelY = mid - 7;
    visitorsLabelY = mid + 7;
  }

  const indexAt = (clientX: number): number | null => {
    const el = wrapRef.current;
    if (!el || daily.length === 0) return null;
    const x = clientX - el.getBoundingClientRect().left;
    const raw = daily.length > 1 ? Math.round((x - PAD.left) / geometry.step) : 0;
    return Math.min(daily.length - 1, Math.max(0, raw));
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (daily.length === 0) return;
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      const delta = e.key === 'ArrowRight' ? 1 : -1;
      setActive((i) => Math.min(daily.length - 1, Math.max(0, (i ?? daily.length - 1) + delta)));
    } else if (e.key === 'Escape') {
      setActive(null);
    }
  };

  const activePoint = active !== null ? geometry.points[active] : undefined;
  const activeDay = active !== null ? daily[active] : undefined;

  return (
    <div
      ref={wrapRef}
      className={styles.chartWrap}
      onMouseMove={(e) => setActive(indexAt(e.clientX))}
      onMouseLeave={() => setActive(null)}
    >
      <svg
        className={styles.chart}
        width={width}
        height={CHART_HEIGHT}
        role="img"
        aria-label={t('analytics.chartLabel')}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onBlur={() => setActive(null)}
      >
        {geometry.ticks.map((tick) => (
          <g key={tick.value}>
            <line
              className={tick.value === 0 ? styles.baseline : styles.gridline}
              x1={PAD.left}
              x2={width - PAD.right}
              y1={tick.y}
              y2={tick.y}
            />
            <text className={styles.axisLabel} x={PAD.left - 8} y={tick.y + 4} textAnchor="end">
              {formatNumber(tick.value)}
            </text>
          </g>
        ))}
        {labelIndexes(daily.length, 5).map((i) => (
          <text
            key={daily[i].day}
            className={styles.axisLabel}
            x={geometry.points[i].x}
            y={CHART_HEIGHT - 8}
            textAnchor={i === 0 ? 'start' : i === daily.length - 1 ? 'end' : 'middle'}
          >
            {formatDay(daily[i].day)}
          </text>
        ))}
        {activePoint && (
          <line
            className={styles.crosshair}
            x1={activePoint.x}
            x2={activePoint.x}
            y1={PAD.top}
            y2={CHART_HEIGHT - PAD.bottom}
          />
        )}
        <path className={cx(styles.line, styles.seriesPageviews)} d={geometry.pageviewsPath} />
        <path className={cx(styles.line, styles.seriesVisitors)} d={geometry.visitorsPath} />
        {activePoint && (
          <>
            <circle
              className={cx(styles.marker, styles.seriesPageviews)}
              cx={activePoint.x}
              cy={activePoint.pageviewsY}
              r={4}
            />
            <circle
              className={cx(styles.marker, styles.seriesVisitors)}
              cx={activePoint.x}
              cy={activePoint.visitorsY}
              r={4}
            />
          </>
        )}
        {last && lastDay && (
          <>
            <text className={styles.endLabel} x={last.x + 8} y={pageviewsLabelY + 4}>
              {t('analytics.pageviews')}
            </text>
            <text className={styles.endLabel} x={last.x + 8} y={visitorsLabelY + 4}>
              {t('analytics.visitors')}
            </text>
          </>
        )}
      </svg>
      {activePoint && activeDay && (
        <div
          className={styles.tooltip}
          role="status"
          style={{
            // Right of the crosshair in the left half, left of it in the right half (keeps the end labels visible)
            left:
              activePoint.x < width / 2
                ? activePoint.x + 12
                : Math.max(0, activePoint.x - 12 - TOOLTIP_WIDTH),
            top: PAD.top,
            width: TOOLTIP_WIDTH,
          }}
        >
          <div className={styles.tooltipDate}>{formatDay(activeDay.day, true)}</div>
          <div className={styles.tooltipRow}>
            <span className={cx(styles.swatch, styles.swatchPageviews)} aria-hidden="true" />
            <span>{t('analytics.pageviews')}</span>
            <strong>{formatNumber(activeDay.pageviews)}</strong>
          </div>
          <div className={styles.tooltipRow}>
            <span className={cx(styles.swatch, styles.swatchVisitors)} aria-hidden="true" />
            <span>{t('analytics.visitors')}</span>
            <strong>{formatNumber(activeDay.visitors)}</strong>
          </div>
        </div>
      )}
    </div>
  );
};

/** A ranked list with a bar for each value's pageviews */
const TopTable: React.FC<{
  title: string;
  column: string;
  rows: AnalyticsTotal[];
  label: (value: string) => string;
}> = ({ title, column, rows, label }) => {
  const { t } = useTranslation();
  const max = Math.max(1, ...rows.map((r) => r.pageviews));
  return (
    <section className={styles.card}>
      <h2 className={styles.cardTitle}>{title}</h2>
      {rows.length === 0 ? (
        <p className={styles.muted}>{t('analytics.noData')}</p>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th scope="col">{column}</th>
              <th scope="col" className={styles.num}>
                {t('analytics.pageviews')}
              </th>
              <th scope="col" className={styles.num}>
                {t('analytics.visitors')}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.value}>
                <td className={styles.nameCell}>
                  <span className={styles.name} title={label(row.value)}>
                    {label(row.value)}
                  </span>
                  <span className={styles.barTrack} aria-hidden="true">
                    <span
                      className={styles.bar}
                      style={{ width: `${(row.pageviews / max) * 100}%` }}
                    />
                  </span>
                </td>
                <td className={styles.num}>{formatNumber(row.pageviews)}</td>
                <td className={styles.num}>{formatNumber(row.visitors)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
};

export const AnalyticsView: React.FC<AnalyticsViewProps> = ({ onShowToast }) => {
  const { t } = useTranslation();
  const [range, setRange] = useState<RangeDays>(30);
  const [summary, setSummary] = useState<AnalyticsSummary | null>(null);
  const [showTable, setShowTable] = useState(false);
  // Each reload or range change is a new request; loading until that request has answered
  const [reloadCount, setReloadCount] = useState(0);
  const requestKey = `${range}:${reloadCount}`;
  const [answeredKey, setAnsweredKey] = useState<string | null>(null);
  const loading = answeredKey !== requestKey;
  // The parent's toast callback may change on every render; reloading must not follow it
  const toastRef = useRef(onShowToast);
  const [exporting, setExporting] = useState(false);

  const handleExportCsv = async () => {
    if (!summary || exporting) return;
    setExporting(true);
    try {
      await aegisApi.downloadAnalyticsCsv({ from: summary.from, to: summary.to });
      toastRef.current?.(t('analytics.exportSuccess'), 'success');
    } catch (e: unknown) {
      toastRef.current?.(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      setExporting(false);
    }
  };

  const renderChange = (pct: number | null | undefined) => {
    if (pct === null || pct === undefined) {
      return (
        <span
          className={cx(styles.changeBadge, styles.changeNeutral)}
          title={t('analytics.previousPeriod')}
        >
          <Minus size={12} />
          <span>--</span>
        </span>
      );
    }
    const isPos = pct > 0;
    const isNeg = pct < 0;
    const Icon = isPos ? TrendingUp : isNeg ? TrendingDown : Minus;
    const badgeClass = isPos
      ? styles.changePositive
      : isNeg
        ? styles.changeNegative
        : styles.changeNeutral;
    const text = `${isPos ? '+' : ''}${pct}%`;
    return (
      <span className={cx(styles.changeBadge, badgeClass)} title={t('analytics.previousPeriod')}>
        <Icon size={12} />
        <span>{text}</span>
      </span>
    );
  };
  useEffect(() => {
    toastRef.current = onShowToast;
  });

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .getAnalyticsSummary({ ...lastDays(range), limit: 10 })
      .then((data) => {
        if (!cancelled) setSummary(data);
      })
      .catch((e: unknown) => {
        if (!cancelled) toastRef.current?.(e instanceof Error ? e.message : String(e), 'error');
      })
      .finally(() => {
        if (!cancelled) setAnsweredKey(`${range}:${reloadCount}`);
      });
    return () => {
      cancelled = true;
    };
  }, [range, reloadCount]);

  const deviceLabel = (value: string) =>
    value === 'desktop' || value === 'mobile' || value === 'tablet'
      ? t(`analytics.deviceNames.${value}`)
      : value;

  return (
    <div className={styles.container}>
      <div className={styles.column}>
        <div className={styles.header}>
          <div className={styles.titleArea}>
            <BarChart3 className={styles.titleIcon} size={26} />
            <h1 className={styles.titleText}>{t('analytics.title')}</h1>
          </div>
          <div className={styles.controls}>
            <div className={styles.rangeTabs} role="group" aria-label={t('analytics.rangeLabel')}>
              {RANGES.map((days) => (
                <button
                  key={days}
                  type="button"
                  aria-pressed={range === days}
                  className={cx(styles.rangeTab, range === days && styles.rangeTabActive)}
                  onClick={() => setRange(days)}
                >
                  {t('analytics.lastDays', { count: days })}
                </button>
              ))}
            </div>
            <Button
              variant="tool"
              onClick={handleExportCsv}
              disabled={loading || exporting || !summary}
            >
              <Download size={14} />
              <span>{t('analytics.exportCsv')}</span>
            </Button>
            <Button variant="tool" onClick={() => setReloadCount((n) => n + 1)} disabled={loading}>
              <RefreshCw className={loading ? 'animate-spin' : ''} size={14} />
              <span>{t('common.reload')}</span>
            </Button>
          </div>
        </div>

        <p className={styles.note}>{t('analytics.privacyNote')}</p>

        {summary && (
          <>
            <div className={styles.tiles}>
              <div className={styles.tile}>
                <div className={styles.tileHeader}>
                  <span className={styles.tileLabel}>{t('analytics.pageviews')}</span>
                  {summary.previous && renderChange(summary.previous.pageviews_change_pct)}
                </div>
                <span className={styles.tileValue}>{formatNumber(summary.pageviews)}</span>
                {summary.previous && (
                  <span className={styles.tileHint}>
                    {t('analytics.previousPeriod')}: {formatNumber(summary.previous.pageviews)}
                  </span>
                )}
              </div>
              <div className={styles.tile}>
                <div className={styles.tileHeader}>
                  <span className={styles.tileLabel}>{t('analytics.visitors')}</span>
                  {summary.previous && renderChange(summary.previous.visitors_change_pct)}
                </div>
                <span className={styles.tileValue}>{formatNumber(summary.visitors)}</span>
                {summary.previous ? (
                  <span className={styles.tileHint}>
                    {t('analytics.previousPeriod')}: {formatNumber(summary.previous.visitors)}
                  </span>
                ) : (
                  <span className={styles.tileHint}>{t('analytics.visitorsHint')}</span>
                )}
              </div>
            </div>

            <section className={styles.card}>
              <div className={styles.cardHead}>
                <h2 className={styles.cardTitle}>{t('analytics.daily')}</h2>
                <div className={styles.legend}>
                  <span className={styles.legendItem}>
                    <span
                      className={cx(styles.swatch, styles.swatchPageviews)}
                      aria-hidden="true"
                    />
                    {t('analytics.pageviews')}
                  </span>
                  <span className={styles.legendItem}>
                    <span className={cx(styles.swatch, styles.swatchVisitors)} aria-hidden="true" />
                    {t('analytics.visitors')}
                  </span>
                  <Button variant="tool" onClick={() => setShowTable((v) => !v)}>
                    {showTable ? t('analytics.showChart') : t('analytics.showTable')}
                  </Button>
                </div>
              </div>
              {summary.pageviews === 0 && <p className={styles.muted}>{t('analytics.empty')}</p>}
              {showTable ? (
                <div className={styles.tableScroll}>
                  <table className={styles.table}>
                    <thead>
                      <tr>
                        <th scope="col">{t('analytics.date')}</th>
                        <th scope="col" className={styles.num}>
                          {t('analytics.pageviews')}
                        </th>
                        <th scope="col" className={styles.num}>
                          {t('analytics.visitors')}
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {[...summary.daily].reverse().map((d) => (
                        <tr key={d.day}>
                          <td>{formatDay(d.day, true)}</td>
                          <td className={styles.num}>{formatNumber(d.pageviews)}</td>
                          <td className={styles.num}>{formatNumber(d.visitors)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <DailyChart daily={summary.daily} />
              )}
            </section>

            <div className={styles.grid}>
              <TopTable
                title={t('analytics.topPages')}
                column={t('analytics.page')}
                rows={summary.top_pages}
                label={(v) => v}
              />
              <TopTable
                title={t('analytics.topReferrers')}
                column={t('analytics.referrer')}
                rows={summary.top_referrers}
                label={(v) => v || t('analytics.direct')}
              />
              <TopTable
                title={t('analytics.devices')}
                column={t('analytics.device')}
                rows={summary.devices}
                label={deviceLabel}
              />
              <TopTable
                title={t('analytics.topUtmSources')}
                column={t('analytics.source')}
                rows={summary.top_utm_sources}
                label={(v) => v}
              />
              <TopTable
                title={t('analytics.topUtmCampaigns')}
                column={t('analytics.campaign')}
                rows={summary.top_utm_campaigns}
                label={(v) => v}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
};
