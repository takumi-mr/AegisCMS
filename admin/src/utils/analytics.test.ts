import { describe, expect, it } from 'vitest';

import { labelIndexes, lastDays, lineChartGeometry, niceMax, utcDay } from './analytics';

describe('analytics utilities', () => {
  it('computes the last N days in UTC', () => {
    const today = new Date(Date.UTC(2026, 9, 29, 23, 30));
    expect(utcDay(today)).toBe('2026-10-29');
    expect(lastDays(7, today)).toEqual({ from: '2026-10-23', to: '2026-10-29' });
    expect(lastDays(1, today)).toEqual({ from: '2026-10-29', to: '2026-10-29' });
    expect(lastDays(30, new Date(Date.UTC(2026, 2, 5)))).toEqual({
      from: '2026-02-04',
      to: '2026-03-05',
    });
  });

  it('rounds the axis maximum up to a round number', () => {
    expect(niceMax(0)).toBe(4);
    expect(niceMax(3)).toBe(5);
    expect(niceMax(10)).toBe(10);
    expect(niceMax(11)).toBe(20);
    expect(niceMax(180)).toBe(200);
    expect(niceMax(2400)).toBe(2500);
  });

  it('lays out both lines on one axis', () => {
    const geometry = lineChartGeometry(
      [
        { day: '2026-10-01', pageviews: 0, visitors: 0 },
        { day: '2026-10-02', pageviews: 10, visitors: 5 },
      ],
      120,
      120,
      { top: 10, right: 10, bottom: 10, left: 10 }
    );
    expect(geometry.pageviewsPath).toBe('M10.0,110.0 L110.0,10.0');
    expect(geometry.visitorsPath).toBe('M10.0,110.0 L110.0,60.0');
    expect(geometry.ticks.map((t) => t.value)).toEqual([0, 2.5, 5, 7.5, 10]);
    expect(geometry.ticks[0].y).toBe(110);
    expect(geometry.step).toBe(100);
  });

  it('centers a single day', () => {
    const geometry = lineChartGeometry(
      [{ day: '2026-10-01', pageviews: 1, visitors: 1 }],
      100,
      100,
      { top: 0, right: 0, bottom: 0, left: 0 }
    );
    expect(geometry.points[0].x).toBe(50);
  });

  it('picks evenly spaced x labels including both ends', () => {
    expect(labelIndexes(3, 5)).toEqual([0, 1, 2]);
    expect(labelIndexes(30, 5)).toEqual([0, 7, 15, 22, 29]);
    expect(labelIndexes(0, 5)).toEqual([]);
  });
});
