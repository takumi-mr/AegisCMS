// Helpers for the analytics dashboard: date ranges and the daily line chart's geometry

import type { AnalyticsDay } from '../types';

/** `YYYY-MM-DD` of a date in UTC (the server counts days in UTC) */
export const utcDay = (date: Date): string => date.toISOString().slice(0, 10);

/** The last `days` days up to `today` (inclusive), as the summary API's `from` / `to` */
export const lastDays = (days: number, today: Date = new Date()): { from: string; to: string } => {
  const from = new Date(today.getTime());
  from.setUTCDate(from.getUTCDate() - (days - 1));
  return { from: utcDay(from), to: utcDay(today) };
};

/** A round axis maximum at or above `value` (1, 2, 2.5, 5 × 10^n), so the gridlines land on round numbers */
export const niceMax = (value: number): number => {
  if (value <= 0) return 4;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  for (const step of [1, 2, 2.5, 5, 10]) {
    if (step * magnitude >= value) return step * magnitude;
  }
  return 10 * magnitude;
};

export interface ChartPoint {
  x: number;
  pageviewsY: number;
  visitorsY: number;
}

export interface LineChartGeometry {
  points: ChartPoint[];
  /** `d` of the pageviews and visitors lines */
  pageviewsPath: string;
  visitorsPath: string;
  /** Gridlines (value and y), from the baseline up */
  ticks: Array<{ value: number; y: number }>;
  /** Width of one day's hover column */
  step: number;
}

/**
 * Lays out the daily pageviews and visitors as two lines on one axis (both are counts of the same
 * kind) inside `width` × `height`, with `pad` around the plot for the axis labels
 */
export const lineChartGeometry = (
  daily: AnalyticsDay[],
  width: number,
  height: number,
  pad: { top: number; right: number; bottom: number; left: number }
): LineChartGeometry => {
  const plotWidth = Math.max(1, width - pad.left - pad.right);
  const plotHeight = Math.max(1, height - pad.top - pad.bottom);
  const max = niceMax(Math.max(0, ...daily.map((d) => d.pageviews)));
  const step = daily.length > 1 ? plotWidth / (daily.length - 1) : plotWidth;
  const y = (value: number) => pad.top + plotHeight - (value / max) * plotHeight;
  const points = daily.map((d, i) => ({
    x: pad.left + (daily.length > 1 ? i * step : plotWidth / 2),
    pageviewsY: y(d.pageviews),
    visitorsY: y(d.visitors),
  }));
  const path = (key: 'pageviewsY' | 'visitorsY') =>
    points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p[key].toFixed(1)}`).join(' ');
  const ticks = [0, 1, 2, 3, 4].map((i) => ({ value: (max / 4) * i, y: y((max / 4) * i) }));
  return {
    points,
    pageviewsPath: path('pageviewsY'),
    visitorsPath: path('visitorsY'),
    ticks,
    step: daily.length > 1 ? step : plotWidth,
  };
};

/** Indexes of the days to label on the x axis: about `count` evenly spaced, always the first and last */
export const labelIndexes = (length: number, count: number): number[] => {
  if (length <= 0) return [];
  if (length <= count) return Array.from({ length }, (_, i) => i);
  const out = new Set<number>();
  for (let i = 0; i < count; i++) {
    out.add(Math.round((i * (length - 1)) / (count - 1)));
  }
  return [...out];
};
