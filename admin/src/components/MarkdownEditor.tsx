import React, { useRef } from 'react';
import {
  Heading,
  Bold,
  Italic,
  Code2,
  List,
  ListOrdered,
  Image as ImageIcon,
  Link,
  Clipboard,
  CheckCircle2,
  Code,
  Table,
  Quote,
  Info,
  ListTree,
  ListCollapse,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from './ui/Button';
import type { BlockNode } from '../types';
import { summarizeBlocks } from '../utils/markdown';
import styles from './MarkdownEditor.module.css';

interface MarkdownEditorProps {
  value: string;
  onChange: (value: string) => void;
  blocks: BlockNode[];
  onSave?: () => void;
}

export const MarkdownEditor: React.FC<MarkdownEditorProps> = ({
  value,
  onChange,
  blocks,
  onSave,
}) => {
  const { t } = useTranslation();
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const insertText = (before: string, after: string = '', defaultText: string = '') => {
    const textarea = textareaRef.current;
    if (!textarea) return;

    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selected = value.substring(start, end) || defaultText;
    const replacement = `${before}${selected}${after}`;

    const nextValue = value.substring(0, start) + replacement + value.substring(end);
    onChange(nextValue);

    // Restore selection inside the inserted tags
    setTimeout(() => {
      textarea.focus();
      textarea.setSelectionRange(start + before.length, start + before.length + selected.length);
    }, 10);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // 1. Tab key for 2 spaces indent
    if (e.key === 'Tab') {
      e.preventDefault();
      const textarea = textareaRef.current;
      if (!textarea) return;
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      const nextValue = value.substring(0, start) + '  ' + value.substring(end);
      onChange(nextValue);
      setTimeout(() => {
        textarea.selectionStart = textarea.selectionEnd = start + 2;
      }, 0);
    }

    // 2. Ctrl+S or Cmd+S for quick save
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      onSave?.();
    }
  };

  const handlePasteClipboard = async () => {
    try {
      if (!navigator.clipboard?.readText) return;
      const text = await navigator.clipboard.readText();
      if (!text) return;
      insertText('', '', text);
    } catch (err) {
      console.warn('Could not read clipboard in MarkdownEditor:', err);
    }
  };

  // Calculate stats
  const lineCount = value.length === 0 ? 0 : value.split('\n').length;
  const wordCount = value.trim().length === 0 ? 0 : value.trim().split(/\s+/).length;
  const charCount = value.length;
  const blocksSummary = summarizeBlocks(blocks);

  return (
    <div className={styles.editorContainer}>
      {/* Quick Markdown Formatting Toolbar */}
      <div className={styles.toolbar} role="toolbar" aria-label={t('markdown.toolbar')}>
        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('## ', '', t('markdown.samples.heading'))}
            title={t('markdown.heading')}
          >
            <Heading size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('**', '**', t('markdown.samples.bold'))}
            title={t('markdown.bold')}
          >
            <Bold size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('*', '*', t('markdown.samples.italic'))}
            title={t('markdown.italic')}
          >
            <Italic size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('`', '`', t('markdown.samples.inlineCode'))}
            title={t('markdown.inlineCode')}
          >
            <Code size={14} />
          </Button>
        </div>

        <div className={styles.divider} />

        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('```rust\n', '\n```', t('markdown.samples.code'))}
            title={t('markdown.code')}
          >
            <Code2 size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('- ', '', t('markdown.samples.listItem'))}
            title={t('markdown.bullet')}
          >
            <List size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('1. ', '', t('markdown.samples.numberedItem'))}
            title={t('markdown.numbered')}
          >
            <ListOrdered size={14} />
          </Button>
        </div>

        <div className={styles.divider} />

        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() =>
              insertText(
                `| ${t('markdown.samples.column', { n: 1 })} | ${t('markdown.samples.column', { n: 2 })} |\n| --- | --- |\n| `,
                ' |  |\n',
                t('markdown.samples.cell')
              )
            }
            title={t('markdown.table')}
          >
            <Table size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('> ', '', t('markdown.samples.quote'))}
            title={t('markdown.quote')}
          >
            <Quote size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('> [!NOTE]\n> ', '', t('markdown.samples.callout'))}
            title={t('markdown.callout')}
          >
            <Info size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() =>
              insertText(
                `:::details ${t('markdown.samples.detailsSummary')}\n`,
                '\n:::\n',
                t('markdown.samples.detailsBody')
              )
            }
            title={t('markdown.details')}
          >
            <ListCollapse size={14} />
          </Button>
          <Button variant="icon" onClick={() => insertText('[TOC]\n')} title={t('markdown.toc')}>
            <ListTree size={14} />
          </Button>
        </div>

        <div className={styles.divider} />

        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('![', '](https://)', t('markdown.samples.alt'))}
            title={t('markdown.image')}
          >
            <ImageIcon size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('[', '](https://)', t('markdown.samples.link'))}
            title={t('markdown.link')}
          >
            <Link size={14} />
          </Button>
          <Button variant="icon" onClick={handlePasteClipboard} title={t('markdown.paste')}>
            <Clipboard size={14} />
          </Button>
        </div>
      </div>

      {/* Editor Main Textarea */}
      <div className={styles.editorAreaWrapper}>
        <textarea
          ref={textareaRef}
          className={styles.textarea}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t('markdown.placeholder')}
          spellCheck={false}
        />
      </div>

      {/* Status Bar */}
      <div className={styles.statusBar}>
        <div className={styles.statusLeft}>
          <span>{t('markdown.lines', { count: lineCount })}</span>
          <span>{t('markdown.words', { count: wordCount })}</span>
          <span>{t('markdown.chars', { count: charCount })}</span>
          <span className={styles.tag}>
            {t('markdown.astBlocks', { count: blocks.length, summary: blocksSummary })}
          </span>
        </div>
        <div className={styles.statusRight}>
          <CheckCircle2 size={13} />
          <span>{t('markdown.synced')}</span>
        </div>
      </div>
    </div>
  );
};
