import { describe, expect, it } from 'vitest';

import type { FieldDefinition, SchemaDefinition } from '../types';
import { extraFieldDefs, fromExtraValues, toExtraValues } from './extraFields';

const def = (
  name: string,
  type: string,
  extra: Partial<FieldDefinition> = {}
): FieldDefinition => ({
  name,
  type,
  required: false,
  nullable: false,
  unique: false,
  index: false,
  indexed_paths: [],
  ...extra,
});

const schema = (contentType: string, fields: FieldDefinition[]): SchemaDefinition => ({
  content_type: contentType,
  label: contentType,
  fields,
});

describe('extraFieldDefs', () => {
  it('leaves out category_id and the fields with a dedicated input', () => {
    const article = schema('article', [
      def('category_id', 'relation'),
      def('excerpt', 'string'),
      def('custom_meta', 'jsonb'),
    ]);
    expect(extraFieldDefs(article, 'article').map((f) => f.name)).toEqual([
      'excerpt',
      'custom_meta',
    ]);

    const page = schema('page', [def('layout', 'string'), def('custom_meta', 'jsonb')]);
    expect(extraFieldDefs(page, 'page').map((f) => f.name)).toEqual(['custom_meta']);

    const product = schema('product', [def('price', 'integer'), def('weight', 'integer')]);
    expect(extraFieldDefs(product, 'product').map((f) => f.name)).toEqual(['weight']);
  });

  it('keeps every field of a type that has no dedicated inputs', () => {
    const event = schema('event', [def('category_id', 'relation'), def('venue', 'string')]);
    expect(extraFieldDefs(event, 'event').map((f) => f.name)).toEqual(['venue']);
    expect(extraFieldDefs(undefined, 'event')).toEqual([]);
  });
});

describe('toExtraValues / fromExtraValues', () => {
  const defs = [
    def('note', 'string', { nullable: true }),
    def('count', 'integer', { nullable: true }),
    def('featured', 'boolean'),
    def('meta', 'jsonb', { nullable: true }),
  ];

  it('round-trips stored values', () => {
    const stored = { note: 'hi', count: 3, featured: true, meta: { a: 1 } };
    expect(fromExtraValues(defs, toExtraValues(defs, stored))).toEqual({ fields: stored });
  });

  it('sends null for empty nullable fields and leaves out the other empty ones', () => {
    const all = [...defs, def('plain', 'string')];
    expect(fromExtraValues(all, toExtraValues(all, {}))).toEqual({
      fields: { note: null, count: null, featured: false, meta: null },
    });
  });

  it('reports the first value that cannot be saved', () => {
    expect(fromExtraValues(defs, { note: '', count: '1.5', featured: false, meta: '' })).toEqual({
      error: { field: 'count', code: 'integer' },
    });
    expect(fromExtraValues(defs, { note: '', count: '', featured: false, meta: '{' })).toEqual({
      error: { field: 'meta', code: 'json' },
    });
    expect(fromExtraValues([def('name2', 'string', { required: true })], { name2: ' ' })).toEqual({
      error: { field: 'name2', code: 'required' },
    });
  });
});
