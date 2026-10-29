import type { BlockNode, LocalizedText, PluginBlockDefinition } from '../types';

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

/** Attributes of a newly placed block: each field's default */
export function defaultAttributes(definition: PluginBlockDefinition): Record<string, unknown> {
  const attributes: Record<string, unknown> = {};
  for (const field of definition.fields) {
    if (field.default !== undefined) attributes[field.name] = field.default;
  }
  return attributes;
}

/** Names of the required fields the block leaves empty */
export function missingRequiredFields(
  definition: PluginBlockDefinition,
  block: BlockNode
): string[] {
  return definition.fields
    .filter((f) => f.required)
    .filter((f) => {
      const value = block.attributes?.[f.name];
      return value == null || (typeof value === 'string' && value.trim() === '');
    })
    .map((f) => f.name);
}
