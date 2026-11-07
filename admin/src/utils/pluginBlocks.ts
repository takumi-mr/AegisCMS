import type { BlockNode, LocalizedText, PluginBlockDefinition, PluginBlockField } from '../types';

/** The most items a `list` field can hold when the manifest sets no `max_items` */
export const MAX_LIST_ITEMS = 100;

/** A block of a plugin (`<plugin>/<block>`), not one of the built-in `core/` blocks */
export function isPluginBlockName(name: string): boolean {
  return /^[a-z0-9][a-z0-9_-]*\/[a-z0-9][a-z0-9_-]*$/.test(name) && !name.startsWith('core/');
}

/** Text of a manifest in `language` (e.g. `ja-JP` → `ja`), else English, else the first one */
export function localize(text: LocalizedText | undefined, language: string): string {
  if (text == null) return '';
  if (typeof text === 'string') return text;
  const short = language.split('-')[0];
  return text[language] ?? text[short] ?? text.en ?? Object.values(text)[0] ?? '';
}

/** Values of a new block, or of a new item of a `list`: each field's default */
export function defaultValues(fields: PluginBlockField[]): Record<string, unknown> {
  const values: Record<string, unknown> = {};
  for (const field of fields) {
    if (field.default !== undefined) values[field.name] = field.default;
  }
  return values;
}

/** Attributes of a newly placed block: each field's default */
export function defaultAttributes(definition: PluginBlockDefinition): Record<string, unknown> {
  return defaultValues(definition.fields);
}

/** The items of a `list` field's value (anything that is not an object is dropped) */
export function listItems(value: unknown): Record<string, unknown>[] {
  if (!Array.isArray(value)) return [];
  return value.filter(
    (item): item is Record<string, unknown> =>
      item != null && typeof item === 'object' && !Array.isArray(item)
  );
}

/** Whether an item of a `list` has nothing typed in (a select or a switch alone does not count) */
export function isBlankItem(fields: PluginBlockField[], item: Record<string, unknown>): boolean {
  return fields
    .filter((f) => f.type !== 'select' && f.type !== 'boolean')
    .every((f) => isEmptyValue(f, item[f.name]));
}

/** Whether a field's value counts as "left empty" */
function isEmptyValue(field: PluginBlockField, value: unknown): boolean {
  if (field.type === 'list') {
    return listItems(value).every((item) => isBlankItem(field.fields ?? [], item));
  }
  return value == null || (typeof value === 'string' && value.trim() === '');
}

/** Names of the required fields in `values` (a block's attributes, or an item of a list) left empty */
export function missingRequired(
  fields: PluginBlockField[],
  values: Record<string, unknown> | undefined
): string[] {
  return fields.filter((f) => f.required && isEmptyValue(f, values?.[f.name])).map((f) => f.name);
}

/** Names of the required fields the block leaves empty */
export function missingRequiredFields(
  definition: PluginBlockDefinition,
  block: BlockNode
): string[] {
  return missingRequired(definition.fields, block.attributes);
}
