import { describe, expect, it } from 'vitest';

import { formatDate, formatDateTime, toDateTimeLocal } from './format';

describe('format utilities', () => {
  describe('toDateTimeLocal', () => {
    it('formats a date into local YYYY-MM-DDTHH:mm input format', () => {
      // 2026-10-25 09:05:00
      const d = new Date(2026, 9, 25, 9, 5, 0); // month is 0-indexed: 9 = October
      const localStr = toDateTimeLocal(d.toISOString());
      expect(localStr).toBe('2026-10-25T09:05');
    });

    it('returns empty string for invalid date', () => {
      expect(toDateTimeLocal('not-a-valid-date')).toBe('');
      expect(toDateTimeLocal('')).toBe('');
    });
  });

  describe('formatDateTime and formatDate', () => {
    it('returns original string when input is not a valid date', () => {
      expect(formatDateTime('invalid')).toBe('invalid');
      expect(formatDate('invalid')).toBe('invalid');
    });

    it('formats valid ISO dates without crashing', () => {
      const iso = '2026-10-25T12:30:00Z';
      expect(formatDateTime(iso)).not.toBe('');
      expect(formatDate(iso)).not.toBe('');
      expect(formatDateTime(iso)).toContain('2026');
    });
  });
});
