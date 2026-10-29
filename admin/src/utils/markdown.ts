import i18n from '../i18n';
import type { BlockNode } from '../types';
import { isPluginBlockName } from './pluginBlocks';

/** Kinds of `core/callout`, as in GitHub's alerts (`> [!NOTE]`) */
export const CALLOUT_VARIANTS = ['note', 'tip', 'important', 'warning', 'caution'] as const;
export type CalloutVariant = (typeof CALLOUT_VARIANTS)[number];

/** Deepest heading level a `core/toc` lists by default (H2 and H3) */
export const TOC_DEFAULT_MAX_LEVEL = 3;

type Align = 'left' | 'center' | 'right' | null;

/** Rows of a `core/table` block (always strings) */
export function tableRows(block: BlockNode): string[][] {
  const rows = Array.isArray(block.attributes?.rows) ? block.attributes.rows : [];
  return rows.map((row: unknown) =>
    Array.isArray(row) ? row.map((cell) => (cell == null ? '' : String(cell))) : []
  );
}

function escapeCell(text: string): string {
  return text.replace(/\|/g, '\\|').replace(/\r?\n/g, ' ');
}

function tableToMarkdown(block: BlockNode): string {
  const rows = tableRows(block);
  const columns = Math.max(0, ...rows.map((r) => r.length));
  if (columns === 0) return '';
  const header = block.attributes?.header !== false;
  const align: Align[] = Array.isArray(block.attributes?.align) ? block.attributes.align : [];
  const line = (cells: string[]) =>
    `| ${Array.from({ length: columns }, (_, i) => escapeCell(cells[i] ?? '')).join(' | ')} |`;
  const separator = `| ${Array.from({ length: columns }, (_, i) => {
    switch (align[i]) {
      case 'left':
        return ':---';
      case 'center':
        return ':---:';
      case 'right':
        return '---:';
      default:
        return '---';
    }
  }).join(' | ')} |`;
  // Markdown tables always have a header row; an empty one means "no header" (`header: false`)
  const head = header ? rows[0] : [];
  const body = header ? rows.slice(1) : rows;
  return [line(head), separator, ...body.map(line)].join('\n');
}

/** Splits a Markdown table row into its cells (`\|` is a literal pipe) */
function splitTableRow(line: string): string[] {
  let row = line.trim();
  if (row.startsWith('|')) row = row.slice(1);
  if (row.endsWith('|') && !row.endsWith('\\|')) row = row.slice(0, -1);
  const cells: string[] = [];
  let current = '';
  for (let i = 0; i < row.length; i++) {
    if (row[i] === '\\' && row[i + 1] === '|') {
      current += '|';
      i++;
    } else if (row[i] === '|') {
      cells.push(current.trim());
      current = '';
    } else {
      current += row[i];
    }
  }
  cells.push(current.trim());
  return cells;
}

const TABLE_SEPARATOR = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;

function isTableStart(lines: string[], i: number): boolean {
  return /^\s*\|/.test(lines[i]) && i + 1 < lines.length && TABLE_SEPARATOR.test(lines[i + 1]);
}

function parseAlign(cell: string): Align {
  const c = cell.trim();
  const left = c.startsWith(':');
  const right = c.endsWith(':');
  if (left && right) return 'center';
  if (right) return 'right';
  if (left) return 'left';
  return null;
}

/** `[TOC]`, optionally with `title="..."` and `maxLevel=N` */
const TOC_LINE = /^\[(?:TOC|toc)((?:\s+(?:title="[^"]*"|maxLevel=\d))*)\s*\]$/;

function tocToMarkdown(block: BlockNode): string {
  let options = '';
  const title = String(block.attributes?.title ?? '').trim();
  if (title) options += ` title="${title.replace(/"/g, "'")}"`;
  const maxLevel = Number(block.attributes?.maxLevel ?? TOC_DEFAULT_MAX_LEVEL);
  if (maxLevel !== TOC_DEFAULT_MAX_LEVEL) options += ` maxLevel=${maxLevel}`;
  return `[TOC${options}]`;
}

/** Prefixes every line with `> ` (blank lines become `>`) */
function quoteLines(text: string): string[] {
  return text.split('\n').map((l) => (l.trim() ? `> ${l}` : '>'));
}

/** Last line of a quote naming its source: `— Name` (or `-- Name`) */
const CITE_LINE = /^(?:—|--)\s+(.+)$/;
const CALLOUT_LINE = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*(.*)$/i;
const SEPARATOR_LINE = /^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/;

/**
 * Serializes an array of AST BlockNodes into a clean, human-readable Markdown string.
 */
export function blocksToMarkdown(blocks: BlockNode[]): string {
  const parts: string[] = [];

  for (const block of blocks) {
    switch (block.name) {
      case 'core/heading': {
        const level = Math.max(1, Math.min(6, Number(block.attributes?.level ?? 2)));
        const text = (block.text ?? '').trim();
        parts.push(`${'#'.repeat(level)} ${text}`);
        break;
      }
      case 'core/paragraph': {
        const text = (block.text ?? '').trim();
        if (text) {
          parts.push(text);
        }
        break;
      }
      case 'core/code': {
        const lang = (block.attributes?.language ?? 'rust').trim();
        const code = block.text ?? '';
        parts.push(`\`\`\`${lang}\n${code}\n\`\`\``);
        break;
      }
      case 'core/html': {
        const code = block.text ?? '';
        const fence = block.attributes?.mode === 'sandbox' ? 'html:sandbox' : 'html';
        parts.push(`\`\`\`${fence}\n${code}\n\`\`\``);
        break;
      }
      case 'core/image': {
        const src = (block.attributes?.src ?? '').trim();
        const alt = (block.attributes?.alt ?? '').trim();
        if (src) {
          parts.push(`![${alt}](${src})`);
        }
        break;
      }
      case 'core/table': {
        const table = tableToMarkdown(block);
        if (table) parts.push(table);
        break;
      }
      case 'core/toc': {
        parts.push(tocToMarkdown(block));
        break;
      }
      case 'core/quote': {
        const text = (block.text ?? '').trim();
        const cite = String(block.attributes?.cite ?? '').trim();
        if (text || cite) {
          const lines = quoteLines(text);
          if (cite) lines.push(`> — ${cite}`);
          parts.push(lines.join('\n'));
        }
        break;
      }
      case 'core/separator': {
        parts.push('---');
        break;
      }
      case 'core/callout': {
        const variant = CALLOUT_VARIANTS.includes(block.attributes?.variant)
          ? (block.attributes.variant as CalloutVariant)
          : 'note';
        const title = String(block.attributes?.title ?? '').trim();
        const text = (block.text ?? '').trim();
        const lines = [`> [!${variant.toUpperCase()}]${title ? ` ${title}` : ''}`];
        if (text) lines.push(...quoteLines(text));
        parts.push(lines.join('\n'));
        break;
      }
      case 'core/list': {
        const ordered = Boolean(block.attributes?.ordered);
        const items = block.children ?? [];
        if (items.length > 0) {
          const listLines = items.map((child, i) => {
            const prefix = ordered ? `${i + 1}. ` : '- ';
            return `${prefix}${(child.text ?? '').trim()}`;
          });
          parts.push(listLines.join('\n'));
        }
        break;
      }
      default: {
        // A plugin's block: `::: <plugin>/<block> {"attribute": "value"}` (attributes only)
        if (isPluginBlockName(block.name)) {
          const attributes = block.attributes ?? {};
          parts.push(
            Object.keys(attributes).length > 0
              ? `::: ${block.name} ${JSON.stringify(attributes)}`
              : `::: ${block.name}`
          );
          break;
        }
        const text = (block.text ?? '').trim();
        if (text) {
          parts.push(text);
        }
        break;
      }
    }
  }

  return parts.join('\n\n');
}

/**
 * Parses a Markdown string into an array of AST BlockNodes.
 */
export function markdownToBlocks(markdown: string): BlockNode[] {
  const normalized = markdown.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
  const lines = normalized.split('\n');
  const blocks: BlockNode[] = [];

  let i = 0;
  while (i < lines.length) {
    const line = lines[i];

    // 1. Skip completely empty lines
    if (line.trim().length === 0) {
      i++;
      continue;
    }

    // 2. Code Block: ```lang ... ```
    const codeFenceMatch = line.match(/^```([\w:]+)?/);
    if (codeFenceMatch) {
      const language = codeFenceMatch[1] || 'text';
      const codeLines: string[] = [];
      i++; // Move past opening fence
      while (i < lines.length && !lines[i].startsWith('```')) {
        codeLines.push(lines[i]);
        i++;
      }
      if (i < lines.length && lines[i].startsWith('```')) {
        i++; // Move past closing fence
      }
      const langLower = language.toLowerCase();
      if (langLower === 'html' || langLower === 'raw') {
        blocks.push({
          name: 'core/html',
          attributes: {},
          text: codeLines.join('\n'),
        });
      } else if (langLower === 'html:sandbox') {
        blocks.push({
          name: 'core/html',
          attributes: { mode: 'sandbox' },
          text: codeLines.join('\n'),
        });
      } else {
        blocks.push({
          name: 'core/code',
          attributes: { language },
          text: codeLines.join('\n'),
        });
      }
      continue;
    }

    // Table of contents: [TOC]
    const tocMatch = line.trim().match(TOC_LINE);
    if (tocMatch) {
      const attributes: Record<string, any> = {};
      const title = tocMatch[1].match(/title="([^"]*)"/);
      if (title && title[1].trim()) attributes.title = title[1].trim();
      const maxLevel = tocMatch[1].match(/maxLevel=(\d)/);
      attributes.maxLevel = maxLevel
        ? Math.max(2, Math.min(6, Number(maxLevel[1])))
        : TOC_DEFAULT_MAX_LEVEL;
      blocks.push({ name: 'core/toc', attributes });
      i++;
      continue;
    }

    // Separator: --- / *** / ___
    if (SEPARATOR_LINE.test(line)) {
      blocks.push({ name: 'core/separator', attributes: {} });
      i++;
      continue;
    }

    // Table: | a | b | followed by | --- | --- |
    if (isTableStart(lines, i)) {
      const head = splitTableRow(lines[i]);
      const align = splitTableRow(lines[i + 1]).map(parseAlign);
      const body: string[][] = [];
      i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) {
        body.push(splitTableRow(lines[i]));
        i++;
      }
      const header = head.some((cell) => cell.length > 0);
      const columns = Math.max(head.length, ...body.map((r) => r.length));
      const pad = (row: string[]) => Array.from({ length: columns }, (_, c) => row[c] ?? '');
      blocks.push({
        name: 'core/table',
        attributes: {
          header,
          align: Array.from({ length: columns }, (_, c) => align[c] ?? null),
          rows: (header ? [head, ...body] : body).map(pad),
        },
      });
      continue;
    }

    // A plugin's block: ::: <plugin>/<block> {"attribute": "value"} (one line)
    const pluginMatch = line.match(
      /^:::\s*([a-z0-9][a-z0-9_-]*\/[a-z0-9][a-z0-9_-]*)\s*(\{.*\})?\s*$/
    );
    if (pluginMatch && isPluginBlockName(pluginMatch[1])) {
      let attributes: Record<string, any> | null = {};
      if (pluginMatch[2]) {
        try {
          const parsed = JSON.parse(pluginMatch[2]);
          attributes =
            parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : null;
        } catch {
          attributes = null;
        }
      }
      // Attributes that are not a JSON object stay as a paragraph so nothing is lost
      if (attributes) {
        blocks.push({ name: pluginMatch[1], attributes });
        i++;
        continue;
      }
    }

    // Callout (:::note Title ... :::)
    const fenceCallout = line.match(/^:::\s*(note|tip|important|warning|caution)\b\s*(.*)$/i);
    if (fenceCallout) {
      const bodyLines: string[] = [];
      i++;
      while (i < lines.length && !/^:::\s*$/.test(lines[i])) {
        bodyLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++; // closing :::
      const attributes: Record<string, any> = { variant: fenceCallout[1].toLowerCase() };
      if (fenceCallout[2].trim()) attributes.title = fenceCallout[2].trim();
      blocks.push({ name: 'core/callout', attributes, text: bodyLines.join('\n').trim() });
      continue;
    }

    // Blockquote (> ...), or a callout when it starts with > [!NOTE] etc.
    if (/^\s*>/.test(line)) {
      const quoted: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) {
        quoted.push(lines[i].replace(/^\s*>\s?/, ''));
        i++;
      }
      const callout = quoted[0].trim().match(CALLOUT_LINE);
      if (callout) {
        const attributes: Record<string, any> = { variant: callout[1].toLowerCase() };
        if (callout[2].trim()) attributes.title = callout[2].trim();
        blocks.push({
          name: 'core/callout',
          attributes,
          text: quoted.slice(1).join('\n').trim(),
        });
        continue;
      }
      const attributes: Record<string, any> = {};
      const last = quoted[quoted.length - 1]?.trim().match(CITE_LINE);
      if (last && quoted.length > 1) {
        attributes.cite = last[1].trim();
        quoted.pop();
      }
      blocks.push({ name: 'core/quote', attributes, text: quoted.join('\n').trim() });
      continue;
    }

    // 3. Heading: # ... ######
    const headingMatch = line.match(/^(#{1,6})\s+(.*)$/);
    if (headingMatch) {
      const level = headingMatch[1].length;
      const text = headingMatch[2].trim();
      blocks.push({
        name: 'core/heading',
        attributes: { level },
        text,
      });
      i++;
      continue;
    }

    // 4. Image: ![alt](src)
    const imageMatch = line.match(/^!\[(.*?)\]\((.*?)\)$/);
    if (imageMatch) {
      const alt = imageMatch[1].trim();
      const src = imageMatch[2].trim();
      blocks.push({
        name: 'core/image',
        attributes: { src, alt },
      });
      i++;
      continue;
    }

    // 5. Ordered or Unordered List
    const unorderedMatch = line.match(/^[-*+]\s+(.*)$/);
    const orderedMatch = line.match(/^\d+\.\s+(.*)$/);
    if (unorderedMatch || orderedMatch) {
      const ordered = Boolean(orderedMatch);
      const items: BlockNode[] = [];

      while (i < lines.length) {
        const curLine = lines[i];
        if (curLine.trim().length === 0) {
          break;
        }
        const itemMatch = ordered
          ? curLine.match(/^\d+\.\s+(.*)$/)
          : curLine.match(/^[-*+]\s+(.*)$/);
        if (!itemMatch) {
          break;
        }
        items.push({
          name: 'core/list-item',
          attributes: {},
          text: itemMatch[1].trim(),
        });
        i++;
      }

      blocks.push({
        name: 'core/list',
        attributes: { ordered },
        children: items,
      });
      continue;
    }

    // 6. Paragraph (accumulate consecutive non-empty lines until empty line or block marker)
    const paragraphLines: string[] = [];
    while (i < lines.length) {
      const curLine = lines[i];
      if (curLine.trim().length === 0) {
        break;
      }
      // Stop at the start of another block. The first line is always taken: a line no block
      // above recognized (e.g. `:::` with nothing valid after it) is text, and would otherwise
      // never be consumed
      if (
        paragraphLines.length > 0 &&
        (curLine.match(/^```/) ||
          curLine.match(/^(#{1,6})\s+/) ||
          curLine.match(/^!\[(.*?)\]\((.*?)\)$/) ||
          curLine.match(/^[-*+]\s+/) ||
          curLine.match(/^(\d+)\.\s+/) ||
          curLine.match(/^\s*>/) ||
          curLine.match(/^:::/) ||
          curLine.trim().match(TOC_LINE) ||
          SEPARATOR_LINE.test(curLine) ||
          isTableStart(lines, i))
      ) {
        break;
      }
      paragraphLines.push(curLine);
      i++;
    }

    if (paragraphLines.length > 0) {
      blocks.push({
        name: 'core/paragraph',
        attributes: {},
        text: paragraphLines.join('\n'),
      });
    }
  }

  return blocks;
}

/**
 * Returns true if the string looks like structured Markdown (headings, code, lists, images, or multiline content).
 */
export function isMarkdownLike(text: string): boolean {
  if (!text || text.trim().length === 0) return false;
  const trimmed = text.trim();
  // Check for markdown syntax patterns
  if (
    /^#{1,6}\s+/m.test(trimmed) ||
    /^```/m.test(trimmed) ||
    /^[-*+]\s+/m.test(trimmed) ||
    /^\d+\.\s+/m.test(trimmed) ||
    /!\[.*?\]\(.*?\)/.test(trimmed) ||
    /^\s*>/m.test(trimmed) ||
    /^\|.*\|\s*$/m.test(trimmed) ||
    trimmed.includes('\n\n')
  ) {
    return true;
  }
  return false;
}

/** Translation keys of the block names in `summarizeBlocks` (headings show their level instead) */
const SUMMARY_KEYS = {
  'core/paragraph': 'markdown.summary.paragraph',
  'core/code': 'markdown.summary.code',
  'core/image': 'markdown.summary.image',
  'core/list': 'markdown.summary.list',
  'core/table': 'markdown.summary.table',
  'core/toc': 'markdown.summary.toc',
  'core/quote': 'markdown.summary.quote',
  'core/separator': 'markdown.summary.separator_block',
  'core/callout': 'markdown.summary.callout',
} as const;

/**
 * Returns a human-friendly summary of the parsed blocks (e.g. "1 H2, 3 Paragraph, 1 Code"),
 * in the Admin Studio language.
 */
export function summarizeBlocks(blocks: BlockNode[]): string {
  if (blocks.length === 0) return i18n.t('markdown.summary.empty');
  const counts: Record<string, number> = {};
  for (const b of blocks) {
    const key = SUMMARY_KEYS[b.name as keyof typeof SUMMARY_KEYS];
    const label =
      b.name === 'core/heading'
        ? `H${b.attributes?.level ?? 2}`
        : key
          ? i18n.t(key)
          : b.name.replace('core/', '');
    counts[label] = (counts[label] ?? 0) + 1;
  }
  return Object.entries(counts)
    .map(([name, count]) => i18n.t('markdown.summary.item', { name, count }))
    .join(i18n.t('markdown.summary.separator'));
}
