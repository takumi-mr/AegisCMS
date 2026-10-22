import React, { useState } from 'react';
import { Clipboard, FileCode, Check, AlertCircle } from 'lucide-react';
import { Modal } from './ui/Modal';
import { Button } from './ui/Button';
import { markdownToBlocks, summarizeBlocks } from '../utils/markdown';
import type { BlockNode } from '../types';
import styles from './PasteMarkdownModal.module.css';

interface PasteMarkdownModalProps {
  onInsert: (blocks: BlockNode[], mode: 'append' | 'replace') => void;
  onClose: () => void;
}

export const PasteMarkdownModal: React.FC<PasteMarkdownModalProps> = ({ onInsert, onClose }) => {
  const [markdown, setMarkdown] = useState<string>('');
  const [mode, setMode] = useState<'append' | 'replace'>('append');
  const [copiedState, setCopiedState] = useState<boolean>(false);
  const [errorNotice, setErrorNotice] = useState<string | null>(null);

  const parsedBlocks = markdownToBlocks(markdown);
  const summary = summarizeBlocks(parsedBlocks);

  const handleReadClipboard = async () => {
    try {
      if (!navigator.clipboard?.readText) {
        setErrorNotice('Clipboard access is not permitted in this browser.');
        return;
      }
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setErrorNotice('Clipboard is empty.');
        return;
      }
      setMarkdown(text);
      setErrorNotice(null);
      setCopiedState(true);
      setTimeout(() => setCopiedState(false), 2000);
    } catch (err: unknown) {
      console.warn('Could not read clipboard:', err);
      setErrorNotice(
        'Clipboard read failed. Please paste directly using Ctrl+V / Cmd+V into the text area.'
      );
    }
  };

  const handleApply = () => {
    if (parsedBlocks.length === 0) return;
    onInsert(parsedBlocks, mode);
    onClose();
  };

  return (
    <Modal
      title="Paste & Import Markdown"
      icon={<Clipboard size={18} />}
      onClose={onClose}
      wide
      actions={
        <Button variant="tool" onClick={handleReadClipboard} className={styles.pasteButton}>
          {copiedState ? <Check size={14} /> : <Clipboard size={14} />}
          <span>{copiedState ? 'Pasted!' : 'Paste from Clipboard'}</span>
        </Button>
      }
    >
      <div className={styles.container}>
        <p className={styles.intro}>
          Paste Markdown from your clipboard to automatically parse headings, code blocks, lists,
          images, and paragraphs into AegisCMS AST blocks.
        </p>

        {errorNotice && (
          <div className={styles.summaryBar} style={{ borderColor: 'rgba(244, 63, 94, 0.4)' }}>
            <div className={styles.summaryContent} style={{ color: '#fda4af' }}>
              <AlertCircle size={16} />
              <span>{errorNotice}</span>
            </div>
          </div>
        )}

        <textarea
          className={styles.textarea}
          placeholder="Paste or type your Markdown here...&#10;&#10;# Example Heading&#10;Paragraph text with some details...&#10;&#10;```rust&#10;fn main() {}&#10;```"
          value={markdown}
          onChange={(e) => setMarkdown(e.target.value)}
          autoFocus
        />

        <div className={styles.summaryBar}>
          <div className={styles.summaryContent}>
            <FileCode size={16} />
            <span>Parsed Blocks:</span>
            <span className={styles.badge}>{parsedBlocks.length}</span>
            <span>({summary})</span>
          </div>
        </div>

        <div className={styles.optionsRow}>
          <span>Insert Mode:</span>
          <label className={styles.radioLabel}>
            <input
              type="radio"
              name="insertMode"
              value="append"
              checked={mode === 'append'}
              onChange={() => setMode('append')}
            />
            Append to existing blocks
          </label>
          <label className={styles.radioLabel}>
            <input
              type="radio"
              name="insertMode"
              value="replace"
              checked={mode === 'replace'}
              onChange={() => setMode('replace')}
            />
            Replace all blocks
          </label>
        </div>

        <div className={styles.footer}>
          <Button variant="tool" onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            onClick={handleApply}
            disabled={parsedBlocks.length === 0}
            title={parsedBlocks.length === 0 ? 'Enter markdown to convert' : 'Insert parsed blocks'}
          >
            Insert {parsedBlocks.length > 0 ? `${parsedBlocks.length} Blocks` : 'Blocks'}
          </Button>
        </div>
      </div>
    </Modal>
  );
};
