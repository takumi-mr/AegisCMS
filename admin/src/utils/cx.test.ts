import { describe, expect, it } from 'vitest';

import { cx } from './cx';

describe('cx utility', () => {
  it('joins non-empty strings with space', () => {
    expect(cx('btn', 'primary', 'active')).toBe('btn primary active');
  });

  it('filters out falsy values like false, null, undefined, and empty strings', () => {
    expect(cx('btn', false, null, undefined, '', 'outline')).toBe('btn outline');
  });

  it('handles conditional expressions cleanly', () => {
    const isPrimary = true;
    const isLarge = false;
    expect(cx('btn', isPrimary && 'btn-primary', isLarge && 'btn-lg')).toBe('btn btn-primary');
  });

  it('returns empty string when all arguments are falsy', () => {
    expect(cx(false, null, undefined, '')).toBe('');
    expect(cx()).toBe('');
  });
});
