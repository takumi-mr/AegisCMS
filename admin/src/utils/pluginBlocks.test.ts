import { describe, expect, it } from 'vitest';
import type { PluginBlockDefinition } from '../types';
import {
  defaultAttributes,
  isPluginBlockName,
  localize,
  missingRequiredFields,
} from './pluginBlocks';

const definition: PluginBlockDefinition = {
  name: 'cards/card',
  plugin: 'cards',
  plugin_display_name: 'Cards',
  label: { en: 'Card', ja: 'カード' },
  fields: [
    { name: 'title', type: 'string', label: 'Title', required: true, default: '' },
    { name: 'style', type: 'select', label: 'Style', default: 'plain', options: [] },
    { name: 'note', type: 'text', label: 'Note' },
  ],
};

describe('plugin blocks', () => {
  it('tells plugin blocks from built-in blocks', () => {
    expect(isPluginBlockName('cards/card')).toBe(true);
    expect(isPluginBlockName('official-blocks/link-card')).toBe(true);
    expect(isPluginBlockName('core/table')).toBe(false);
    expect(isPluginBlockName('paragraph')).toBe(false);
    expect(isPluginBlockName('a/b/c')).toBe(false);
  });

  it('picks the text for the language', () => {
    expect(localize({ en: 'Card', ja: 'カード' }, 'ja')).toBe('カード');
    expect(localize({ en: 'Card', ja: 'カード' }, 'ja-JP')).toBe('カード');
    expect(localize({ en: 'Card', ja: 'カード' }, 'fr')).toBe('Card');
    expect(localize({ de: 'Karte' }, 'fr')).toBe('Karte');
    expect(localize('Card', 'ja')).toBe('Card');
    expect(localize(undefined, 'ja')).toBe('');
  });

  it('fills the defaults and finds empty required fields', () => {
    expect(defaultAttributes(definition)).toEqual({ title: '', style: 'plain' });
    expect(
      missingRequiredFields(definition, { name: 'cards/card', attributes: { title: ' ' } })
    ).toEqual(['title']);
    expect(
      missingRequiredFields(definition, { name: 'cards/card', attributes: { title: 'A' } })
    ).toEqual([]);
  });
});
