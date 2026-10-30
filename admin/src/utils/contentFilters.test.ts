import { describe, expect, it } from 'vitest';
import { isFiltered, NO_FILTERS } from './contentFilters';

describe('isFiltered', () => {
  it('is false without search words or filters', () => {
    expect(isFiltered(NO_FILTERS, true)).toBe(false);
    expect(isFiltered({ ...NO_FILTERS, q: '   ' }, true)).toBe(false);
  });

  it('is true for search words, a status or (articles only) a category', () => {
    expect(isFiltered({ ...NO_FILTERS, q: 'rust' }, false)).toBe(true);
    expect(isFiltered({ ...NO_FILTERS, status: 'draft' }, false)).toBe(true);
    expect(isFiltered({ ...NO_FILTERS, category: 'cat-1' }, true)).toBe(true);
    expect(isFiltered({ ...NO_FILTERS, category: 'none' }, true)).toBe(true);
    // Pages have no categories, so a leftover category value is not a condition
    expect(isFiltered({ ...NO_FILTERS, category: 'cat-1' }, false)).toBe(false);
  });
});
