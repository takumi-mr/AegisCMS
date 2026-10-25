import { describe, expect, it } from 'vitest';

import { formatMoney } from './money';

describe('formatMoney utility', () => {
  it('formats JPY with no decimal digits (zero-decimal currency)', () => {
    const formatted = formatMoney(1500, 'jpy');
    // Depending on locale (e.g. en-US vs ja-JP), contains ¥/￥ and 1,500
    expect(formatted).toMatch(/[¥￥]|JPY/);
    expect(formatted).toContain('1,500');
  });

  it('formats USD in minor units (cents to dollars)', () => {
    const formatted = formatMoney(1500, 'usd');
    expect(formatted).toMatch(/\$|USD/);
    expect(formatted).toContain('15.00');
  });

  it('formats EUR in minor units (cents to euros)', () => {
    const formatted = formatMoney(2450, 'eur');
    expect(formatted).toMatch(/€|EUR/);
    expect(formatted).toMatch(/24\.50|24,50/);
  });

  it('handles zero amounts', () => {
    const formattedJpy = formatMoney(0, 'jpy');
    expect(formattedJpy).toContain('0');

    const formattedUsd = formatMoney(0, 'usd');
    expect(formattedUsd).toContain('0.00');
  });

  it('falls back to string concatenation on invalid/unrecognized currency', () => {
    const formatted = formatMoney(100, 'invalid-curr-xyz');
    expect(formatted).toBe('INVALID-CURR-XYZ 100');
  });
});
