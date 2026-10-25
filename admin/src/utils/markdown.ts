import i18n from '../i18n';
import type { BlockNode } from '../types';

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
        parts.push(`\`\`\`html\n${code}\n\`\`\``);
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
    const codeFenceMatch = line.match(/^```(\w*)/);
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
      if (language.toLowerCase() === 'html' || language.toLowerCase() === 'raw') {
        blocks.push({
          name: 'core/html',
          attributes: {},
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
      // Stop if next line is a code fence, heading, standalone image, or list item
      if (
        curLine.match(/^```/) ||
        curLine.match(/^(#{1,6})\s+/) ||
        curLine.match(/^!\[(.*?)\]\((.*?)\)$/) ||
        curLine.match(/^[-*+]\s+/) ||
        curLine.match(/^(\d+)\.\s+/)
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
    trimmed.includes('\n\n')
  ) {
    return true;
  }
  return false;
}

/**
 * Returns a human-friendly summary of the parsed blocks (e.g. "1 H2, 3 Paragraph, 1 Code"),
 * in the Admin Studio language.
 */
export function summarizeBlocks(blocks: BlockNode[]): string {
  if (blocks.length === 0) return i18n.t('markdown.summary.empty');
  const counts: Record<string, number> = {};
  for (const b of blocks) {
    const label =
      b.name === 'core/heading'
        ? `H${b.attributes?.level ?? 2}`
        : b.name === 'core/paragraph'
          ? i18n.t('markdown.summary.paragraph')
          : b.name === 'core/code'
            ? i18n.t('markdown.summary.code')
            : b.name === 'core/image'
              ? i18n.t('markdown.summary.image')
              : b.name === 'core/list'
                ? i18n.t('markdown.summary.list')
                : b.name.replace('core/', '');
    counts[label] = (counts[label] ?? 0) + 1;
  }
  return Object.entries(counts)
    .map(([name, count]) => i18n.t('markdown.summary.item', { name, count }))
    .join(i18n.t('markdown.summary.separator'));
}
