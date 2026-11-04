import { describe, expect, it } from 'vitest';
import type { BlockNode, PluginBlockDefinition } from '../types';
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

const richDefinition: PluginBlockDefinition = {
  name: 'official-blocks/hero',
  plugin: 'official-blocks',
  plugin_display_name: 'Official Blocks',
  label: { en: 'Hero Banner', ja: 'ヒーローバナー' },
  fields: [
    { name: 'heading', type: 'string', label: 'Heading', required: true, default: 'Welcome' },
    { name: 'full_width', type: 'boolean', label: 'Full Width', default: true },
    { name: 'columns', type: 'number', label: 'Columns', default: 3 },
    { name: 'theme', type: 'select', label: 'Theme', default: 'dark', options: [] },
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

  it('correctly simulates block insertion into content editor blocks list', () => {
    const blocks: BlockNode[] = [
      { name: 'core/heading', attributes: { level: 1 }, text: 'Page Title' },
    ];

    // Simulate clicking a custom block button in Admin Studio: addBlock(definition.name, '', defaultAttributes(definition))
    const insertedBlock: BlockNode = {
      name: richDefinition.name,
      attributes: defaultAttributes(richDefinition),
    };
    const updatedBlocks = [...blocks, insertedBlock];

    expect(updatedBlocks).toHaveLength(2);
    expect(updatedBlocks[1].name).toBe('official-blocks/hero');
    expect(updatedBlocks[1].attributes).toEqual({
      heading: 'Welcome',
      full_width: true,
      columns: 3,
      theme: 'dark',
    });
    expect(missingRequiredFields(richDefinition, updatedBlocks[1])).toEqual([]);
  });
});
