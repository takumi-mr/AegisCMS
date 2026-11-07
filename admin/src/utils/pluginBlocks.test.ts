import { describe, expect, it } from 'vitest';
import type { BlockNode, PluginBlockDefinition } from '../types';
import {
  defaultAttributes,
  defaultValues,
  isBlankItem,
  isPluginBlockName,
  listItems,
  localize,
  missingRequired,
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

const faqDefinition: PluginBlockDefinition = {
  name: 'official-blocks/faq',
  plugin: 'official-blocks',
  plugin_display_name: 'Official Blocks',
  label: 'FAQ',
  fields: [
    {
      name: 'items',
      type: 'list',
      label: 'Items',
      required: true,
      fields: [
        { name: 'question', type: 'string', label: 'Question', required: true },
        { name: 'answer', type: 'text', label: 'Answer' },
        { name: 'tone', type: 'select', label: 'Tone', default: 'plain', options: [] },
      ],
    },
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

  it('handles list fields', () => {
    const list = faqDefinition.fields[0];
    const subFields = list.fields ?? [];
    const block = (items: unknown): BlockNode => ({
      name: 'official-blocks/faq',
      attributes: items === undefined ? {} : { items },
    });

    // a new item starts from the defaults of its inputs
    expect(defaultValues(subFields)).toEqual({ tone: 'plain' });

    // a required list needs one item with something typed in
    expect(missingRequiredFields(faqDefinition, block(undefined))).toEqual(['items']);
    expect(missingRequiredFields(faqDefinition, block([]))).toEqual(['items']);
    expect(
      missingRequiredFields(faqDefinition, block([{ question: ' ', answer: '', tone: 'plain' }]))
    ).toEqual(['items']);
    expect(missingRequiredFields(faqDefinition, block([{ question: 'Q' }]))).toEqual([]);

    // a required input of an item is reported against that item
    expect(missingRequired(subFields, { answer: 'A' })).toEqual(['question']);
    expect(missingRequired(subFields, { question: 'Q' })).toEqual([]);

    // a select or a switch alone does not make an item non-empty
    expect(isBlankItem(subFields, { tone: 'plain' })).toBe(true);
    expect(isBlankItem(subFields, { answer: 'A' })).toBe(false);

    // anything that is not an object is dropped
    expect(listItems([{ a: 1 }, null, 'x', [1], 3])).toEqual([{ a: 1 }]);
    expect(listItems('not a list')).toEqual([]);
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
