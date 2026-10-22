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
} from 'lucide-react';
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
      <div className={styles.toolbar} role="toolbar" aria-label="Markdown formatting">
        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('## ', '', 'Section Title')}
            title="Heading 2 (##)"
          >
            <Heading size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('**', '**', 'bold text')}
            title="Bold (**text**)"
          >
            <Bold size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('*', '*', 'italic text')}
            title="Italic (*text*)"
          >
            <Italic size={14} />
          </Button>
        </div>

        <div className={styles.divider} />

        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('```rust\n', '\n```', '// Code here')}
            title="Code Block (```)"
          >
            <Code2 size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('- ', '', 'List item')}
            title="Unordered List (-)"
          >
            <List size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('1. ', '', 'Numbered item')}
            title="Ordered List (1.)"
          >
            <ListOrdered size={14} />
          </Button>
        </div>

        <div className={styles.divider} />

        <div className={styles.toolbarGroup}>
          <Button
            variant="icon"
            onClick={() => insertText('![', '](https://)', 'Alt description')}
            title="Image (![alt](url))"
          >
            <ImageIcon size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={() => insertText('[', '](https://)', 'Link text')}
            title="Link ([text](url))"
          >
            <Link size={14} />
          </Button>
          <Button variant="icon" onClick={handlePasteClipboard} title="Paste text from clipboard">
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
          placeholder='Write your content in Markdown here...&#10;&#10;## Subheading&#10;Paragraph content with **bold**, *italics*, and links.&#10;&#10;```rust&#10;fn main() {&#10;    println!("Hello AegisCMS");&#10;}&#10;```'
          spellCheck={false}
        />
      </div>

      {/* Status Bar */}
      <div className={styles.statusBar}>
        <div className={styles.statusLeft}>
          <span>{lineCount} lines</span>
          <span>{wordCount} words</span>
          <span>{charCount} characters</span>
          <span className={styles.tag}>
            {blocks.length} AST blocks ({blocksSummary})
          </span>
        </div>
        <div className={styles.statusRight}>
          <CheckCircle2 size={13} />
          <span>Live AST & Preview Synchronized</span>
        </div>
      </div>
    </div>
  );
};
