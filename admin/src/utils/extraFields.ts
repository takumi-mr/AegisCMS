import type { FieldDefinition, SchemaDefinition } from '../types';
import { toDateTimeLocal } from './format';

/** Fields every content type has (the server's `STANDARD_COLUMNS`); never part of a schema's extras */
const STANDARD_FIELDS = new Set([
  'id',
  'content_type',
  'title',
  'slug',
  'status',
  'blocks',
  'blocks_json',
  'rendered_html',
  'author_id',
  'published_at',
  'unpublish_at',
  'created_at',
  'updated_at',
]);

/**
 * Fields that have a dedicated input in the editors (so the generic form must not repeat them).
 * `category_id` is built into every type and has its own selector where categories apply.
 */
const DEDICATED_FIELDS: Record<string, string[]> = {
  article: ['category_id'],
  page: ['category_id', 'layout'],
  product: [
    'category_id',
    'price',
    'compare_at_price',
    'stock',
    'sku',
    'image_url',
    'excerpt',
    'buy_url',
  ],
};

/** Schema fields edited by the generic form: everything without a dedicated input */
export const extraFieldDefs = (
  schema: SchemaDefinition | undefined,
  contentType: string
): FieldDefinition[] => {
  if (!schema) return [];
  const dedicated = new Set(DEDICATED_FIELDS[contentType] ?? ['category_id']);
  return schema.fields.filter((f) => !STANDARD_FIELDS.has(f.name) && !dedicated.has(f.name));
};

/** A value of the form: text for most types, a flag for booleans */
export type ExtraValue = string | boolean;
export type ExtraValues = Record<string, ExtraValue>;

/** Why a form value cannot be saved */
export type ExtraFieldErrorCode = 'required' | 'integer' | 'timestamp' | 'json';
export interface ExtraFieldError {
  field: string;
  code: ExtraFieldErrorCode;
}

/** Form values from the `fields` of a stored item (a missing value is empty) */
export const toExtraValues = (
  defs: FieldDefinition[],
  fields: Record<string, unknown> | undefined
): ExtraValues => {
  const values: ExtraValues = {};
  for (const def of defs) {
    const raw = fields?.[def.name];
    if (def.type === 'boolean') {
      values[def.name] = raw === true || raw === 1;
    } else if (raw === undefined || raw === null) {
      values[def.name] = '';
    } else if (def.type === 'jsonb') {
      values[def.name] = JSON.stringify(raw, null, 2);
    } else if (def.type === 'timestamp') {
      values[def.name] = typeof raw === 'string' ? toDateTimeLocal(raw) : '';
    } else {
      values[def.name] = String(raw);
    }
  }
  return values;
};

/**
 * The `fields` to send for the form values, or the first value that cannot be saved.
 * An empty value is `null` when the field allows it, otherwise it is left out. The server
 * replaces all fields on update, so every extra field is sent back, edited or not.
 */
export const fromExtraValues = (
  defs: FieldDefinition[],
  values: ExtraValues
): { fields: Record<string, unknown> } | { error: ExtraFieldError } => {
  const fields: Record<string, unknown> = {};
  for (const def of defs) {
    const value = values[def.name];
    if (def.type === 'boolean') {
      fields[def.name] = value === true;
      continue;
    }
    const text = typeof value === 'string' ? value.trim() : '';
    if (text === '') {
      if (def.required && !def.nullable) return { error: { field: def.name, code: 'required' } };
      if (def.nullable) fields[def.name] = null;
      continue;
    }
    switch (def.type) {
      case 'integer': {
        const n = Number(text);
        if (!Number.isInteger(n)) return { error: { field: def.name, code: 'integer' } };
        fields[def.name] = n;
        break;
      }
      case 'timestamp': {
        const date = new Date(text);
        if (Number.isNaN(date.getTime())) {
          return { error: { field: def.name, code: 'timestamp' } };
        }
        fields[def.name] = date.toISOString();
        break;
      }
      case 'jsonb':
        try {
          fields[def.name] = JSON.parse(text);
        } catch {
          return { error: { field: def.name, code: 'json' } };
        }
        break;
      default:
        fields[def.name] = text;
    }
  }
  return { fields };
};
