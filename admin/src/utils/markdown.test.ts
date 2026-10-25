import { describe, expect, it } from 'vitest';

import type { BlockNode } from '../types';
import { blocksToMarkdown, isMarkdownLike, markdownToBlocks, summarizeBlocks } from './markdown';

describe('markdown utilities', () => {
  describe('blocksToMarkdown', () => {
    it('converts core/heading blocks with correct heading levels', () => {
      const blocks: BlockNode[] = [
        { name: 'core/heading', attributes: { level: 1 }, text: 'Title Level 1' },
        { name: 'core/heading', attributes: { level: 2 }, text: 'Subheading Level 2' },
        { name: 'core/heading', attributes: { level: 3 }, text: 'Section Level 3' },
        { name: 'core/heading', attributes: { level: 6 }, text: 'Smallest Level 6' },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe(
        '# Title Level 1\n\n## Subheading Level 2\n\n### Section Level 3\n\n###### Smallest Level 6'
      );
    });

    it('clamps heading levels between 1 and 6 and defaults to 2', () => {
      const blocks: BlockNode[] = [
        { name: 'core/heading', attributes: { level: 0 }, text: 'Clamp min' },
        { name: 'core/heading', attributes: { level: 9 }, text: 'Clamp max' },
        { name: 'core/heading', attributes: {}, text: 'Default level' },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe('# Clamp min\n\n###### Clamp max\n\n## Default level');
    });

    it('converts core/paragraph blocks and skips empty text', () => {
      const blocks: BlockNode[] = [
        { name: 'core/paragraph', attributes: {}, text: 'First paragraph with some text.' },
        { name: 'core/paragraph', attributes: {}, text: '   ' }, // Should be skipped
        { name: 'core/paragraph', attributes: {}, text: 'Second paragraph.' },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe('First paragraph with some text.\n\nSecond paragraph.');
    });

    it('converts core/code blocks with language attributes', () => {
      const blocks: BlockNode[] = [
        {
          name: 'core/code',
          attributes: { language: 'typescript' },
          text: 'const x: number = 42;\nconsole.log(x);',
        },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe('```typescript\nconst x: number = 42;\nconsole.log(x);\n```');
    });

    it('converts core/html blocks into html fenced blocks', () => {
      const blocks: BlockNode[] = [
        {
          name: 'core/html',
          attributes: {},
          text: '<div class="custom-lp">\n  <h1>Hero</h1>\n</div>',
        },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe('```html\n<div class="custom-lp">\n  <h1>Hero</h1>\n</div>\n```');
    });

    it('converts core/image blocks with src and alt', () => {
      const blocks: BlockNode[] = [
        {
          name: 'core/image',
          attributes: { src: '/uploads/hero.png', alt: 'Hero Banner' },
        },
      ];

      const markdown = blocksToMarkdown(blocks);
      expect(markdown).toBe('![Hero Banner](/uploads/hero.png)');
    });

    it('converts unordered and ordered core/list blocks', () => {
      const unorderedList: BlockNode = {
        name: 'core/list',
        attributes: { ordered: false },
        children: [
          { name: 'core/list-item', attributes: {}, text: 'Item Alpha' },
          { name: 'core/list-item', attributes: {}, text: 'Item Beta' },
          { name: 'core/list-item', attributes: {}, text: 'Item Gamma' },
        ],
      };

      const orderedList: BlockNode = {
        name: 'core/list',
        attributes: { ordered: true },
        children: [
          { name: 'core/list-item', attributes: {}, text: 'Step 1' },
          { name: 'core/list-item', attributes: {}, text: 'Step 2' },
        ],
      };

      expect(blocksToMarkdown([unorderedList])).toBe('- Item Alpha\n- Item Beta\n- Item Gamma');
      expect(blocksToMarkdown([orderedList])).toBe('1. Step 1\n2. Step 2');
    });
  });

  describe('markdownToBlocks', () => {
    it('parses headings with varying levels', () => {
      const md = '# Main Title\n\n## Sub Title\n\n### Minor Title';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(3);
      expect(blocks[0]).toEqual({
        name: 'core/heading',
        attributes: { level: 1 },
        text: 'Main Title',
      });
      expect(blocks[1]).toEqual({
        name: 'core/heading',
        attributes: { level: 2 },
        text: 'Sub Title',
      });
      expect(blocks[2]).toEqual({
        name: 'core/heading',
        attributes: { level: 3 },
        text: 'Minor Title',
      });
    });

    it('parses code blocks with language', () => {
      const md = '```rust\nfn main() {\n    println!("hello");\n}\n```';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(1);
      expect(blocks[0]).toEqual({
        name: 'core/code',
        attributes: { language: 'rust' },
        text: 'fn main() {\n    println!("hello");\n}',
      });
    });

    it('parses html code blocks into core/html blocks', () => {
      const md = '```html\n<section class="banner"><h1>LP Banner</h1></section>\n```';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(1);
      expect(blocks[0]).toEqual({
        name: 'core/html',
        attributes: {},
        text: '<section class="banner"><h1>LP Banner</h1></section>',
      });
    });

    it('parses images into core/image blocks', () => {
      const md = '![Sample Cover](/assets/cover.jpg)';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(1);
      expect(blocks[0]).toEqual({
        name: 'core/image',
        attributes: { src: '/assets/cover.jpg', alt: 'Sample Cover' },
      });
    });

    it('parses unordered and ordered lists', () => {
      const md = '- First\n- Second\n- Third\n\n1. Numbered One\n2. Numbered Two';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(2);
      expect(blocks[0].name).toBe('core/list');
      expect(blocks[0].attributes?.ordered).toBe(false);
      expect(blocks[0].children).toEqual([
        { name: 'core/list-item', attributes: {}, text: 'First' },
        { name: 'core/list-item', attributes: {}, text: 'Second' },
        { name: 'core/list-item', attributes: {}, text: 'Third' },
      ]);

      expect(blocks[1].name).toBe('core/list');
      expect(blocks[1].attributes?.ordered).toBe(true);
      expect(blocks[1].children).toEqual([
        { name: 'core/list-item', attributes: {}, text: 'Numbered One' },
        { name: 'core/list-item', attributes: {}, text: 'Numbered Two' },
      ]);
    });

    it('normalizes CRLF line endings', () => {
      const md = '# Title\r\n\r\nParagraph text.\r\n\r\n- Item 1\r\n- Item 2';
      const blocks = markdownToBlocks(md);

      expect(blocks).toHaveLength(3);
      expect(blocks[0].text).toBe('Title');
      expect(blocks[1].text).toBe('Paragraph text.');
      expect(blocks[2].name).toBe('core/list');
    });
  });

  describe('isMarkdownLike', () => {
    it('detects markdown features', () => {
      expect(isMarkdownLike('# Heading')).toBe(true);
      expect(isMarkdownLike('```\ncode fence\n```')).toBe(true);
      expect(isMarkdownLike('- list item')).toBe(true);
      expect(isMarkdownLike('* list item')).toBe(true);
      expect(isMarkdownLike('1. ordered item')).toBe(true);
      expect(isMarkdownLike('![image](https://example.com/a.png)')).toBe(true);
      expect(isMarkdownLike('First paragraph\n\nSecond paragraph')).toBe(true);
    });

    it('returns false for plain non-markdown text or empty input', () => {
      expect(isMarkdownLike('Just a simple plain sentence without formatting.')).toBe(false);
      expect(isMarkdownLike('Another plain sentence 12345.')).toBe(false);
      expect(isMarkdownLike('')).toBe(false);
      expect(isMarkdownLike('   ')).toBe(false);
    });
  });

  describe('summarizeBlocks', () => {
    it('returns empty message when blocks array is empty', () => {
      const summary = summarizeBlocks([]);
      expect(summary).toBeTruthy();
    });

    it('summarizes various block types correctly', () => {
      const summary = summarizeBlocks([
        { name: 'core/heading', attributes: { level: 2 } },
        { name: 'core/paragraph', attributes: {} },
        { name: 'core/paragraph', attributes: {} },
        { name: 'core/code', attributes: {} },
        { name: 'core/image', attributes: {} },
        { name: 'core/list', attributes: {} },
      ]);
      expect(summary).toBeTruthy();
      expect(summary).toContain('H2');
    });
  });
});
