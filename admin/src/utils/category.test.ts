import { describe, expect, it } from 'vitest';

import type { ContentItem } from '../types';
import { categoryColor, categoryIdOf } from './category';

const makeItem = (fields: Record<string, unknown> = {}): ContentItem => ({
  id: 'content_123',
  content_type: 'article',
  slug: 'test-article',
  title: 'Test Article',
  status: 'published',
  rendered_html: '<p></p>',
  fields,
  blocks: [],
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
});

describe('category utility', () => {
  describe('categoryColor', () => {
    it('returns color when valid 3-character hex code', () => {
      const item = makeItem({ color: '#fff' });
      expect(categoryColor(item)).toBe('#fff');
    });

    it('returns color when valid 6-character hex code', () => {
      const item = makeItem({ color: '#3b82f6' });
      expect(categoryColor(item)).toBe('#3b82f6');
    });

    it('returns color when valid 8-character hex code (with alpha)', () => {
      const item = makeItem({ color: '#3b82f6cc' });
      expect(categoryColor(item)).toBe('#3b82f6cc');
    });

    it('returns undefined for invalid or non-hex string', () => {
      expect(categoryColor(makeItem({ color: 'blue' }))).toBeUndefined();
      expect(categoryColor(makeItem({ color: 'rgb(255,0,0)' }))).toBeUndefined();
      expect(categoryColor(makeItem({ color: '123456' }))).toBeUndefined();
      expect(categoryColor(makeItem({ color: '' }))).toBeUndefined();
      expect(categoryColor(makeItem({}))).toBeUndefined();
    });
  });

  describe('categoryIdOf', () => {
    it('returns category_id string when present and non-empty', () => {
      const item = makeItem({ category_id: 'cat_news' });
      expect(categoryIdOf(item)).toBe('cat_news');
    });

    it('returns undefined when category_id is empty or missing', () => {
      expect(categoryIdOf(makeItem({ category_id: '' }))).toBeUndefined();
      expect(categoryIdOf(makeItem({}))).toBeUndefined();
      expect(categoryIdOf(makeItem({ category_id: 123 }))).toBeUndefined();
    });
  });
});
