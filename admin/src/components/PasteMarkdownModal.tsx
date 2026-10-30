import React, { useState } from 'react';
import { Clipboard, FileCode, Check, AlertCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
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
  const { t } = useTranslation();
  const [markdown, setMarkdown] = useState<string>('');
  const [mode, setMode] = useState<'append' | 'replace'>('append');
  const [copiedState, setCopiedState] = useState<boolean>(false);
  const [errorNotice, setErrorNotice] = useState<string | null>(null);

  const parsedBlocks = markdownToBlocks(markdown);
  const summary = summarizeBlocks(parsedBlocks);

  const handleReadClipboard = async () => {
    try {
      if (!navigator.clipboard?.readText) {
        setErrorNotice(t('paste.noAccess'));
        return;
      }
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setErrorNotice(t('paste.empty'));
        return;
      }
      setMarkdown(text);
      setErrorNotice(null);
      setCopiedState(true);
      setTimeout(() => setCopiedState(false), 2000);
    } catch (err: unknown) {
      console.warn('Could not read clipboard:', err);
      setErrorNotice(t('paste.readFailed'));
    }
  };

  const handleApply = () => {
    if (parsedBlocks.length === 0) return;
    onInsert(parsedBlocks, mode);
    onClose();
  };

  return (
    <Modal
      title={t('paste.title')}
      icon={<Clipboard size={18} />}
      onClose={onClose}
      wide
      actions={
        <Button variant="tool" onClick={handleReadClipboard} className={styles.pasteButton}>
          {copiedState ? <Check size={14} /> : <Clipboard size={14} />}
          <span>{copiedState ? t('paste.pasted') : t('paste.fromClipboard')}</span>
        </Button>
      }
    >
      <div className={styles.container}>
        <p className={styles.intro}>{t('paste.intro')}</p>

        {errorNotice && (
          <div className={styles.summaryBar} style={{ borderColor: 'rgba(244, 63, 94, 0.4)' }}>
            <div className={styles.summaryContent} style={{ color: 'var(--text-rose)' }}>
              <AlertCircle size={16} />
              <span>{errorNotice}</span>
            </div>
          </div>
        )}

        <textarea
          className={styles.textarea}
          placeholder={t('paste.placeholder')}
          value={markdown}
          onChange={(e) => setMarkdown(e.target.value)}
          autoFocus
        />

        <div className={styles.summaryBar}>
          <div className={styles.summaryContent}>
            <FileCode size={16} />
            <span>{t('paste.parsed')}</span>
            <span className={styles.badge}>{parsedBlocks.length}</span>
            <span>({summary})</span>
          </div>
        </div>

        <div className={styles.optionsRow}>
          <span>{t('paste.mode')}</span>
          <label className={styles.radioLabel}>
            <input
              type="radio"
              name="insertMode"
              value="append"
              checked={mode === 'append'}
              onChange={() => setMode('append')}
            />
            {t('paste.append')}
          </label>
          <label className={styles.radioLabel}>
            <input
              type="radio"
              name="insertMode"
              value="replace"
              checked={mode === 'replace'}
              onChange={() => setMode('replace')}
            />
            {t('paste.replace')}
          </label>
        </div>

        <div className={styles.footer}>
          <Button variant="tool" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={handleApply}
            disabled={parsedBlocks.length === 0}
            title={parsedBlocks.length === 0 ? t('paste.enterMarkdown') : t('paste.insertTitle')}
          >
            {parsedBlocks.length > 0
              ? t('paste.insert', { count: parsedBlocks.length })
              : t('paste.insertNone')}
          </Button>
        </div>
      </div>
    </Modal>
  );
};
