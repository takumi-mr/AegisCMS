import React, { useId, useMemo, useState } from 'react';
import { History, RotateCcw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockNode, ContentVersionDetail } from '../types';
import { diffLines, hasChanges, type DiffLine } from '../utils/diff';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { blocksToMarkdown } from '../utils/markdown';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import { Preview } from './Preview';
import styles from './ContentVersionPreviewModal.module.css';

/** What the editor holds now: the other side of the diff */
export interface CurrentContent {
  title: string;
  slug: string;
  blocks: BlockNode[];
  /** Only the custom fields the editor knows (so a field it does not show is not reported as removed) */
  fields: Record<string, unknown>;
}

interface ContentVersionPreviewModalProps {
  contentType: string;
  version: ContentVersionDetail;
  current: CurrentContent;
  memberLevels?: string[];
  /** False hides the restore button (the content cannot be changed by this user) */
  canRestore?: boolean;
  onRestore: (revision: number) => void;
  onClose: () => void;
}

type Tab = 'preview' | 'diff';

const asText = (value: unknown): string =>
  value === undefined || value === null || value === ''
    ? ''
    : typeof value === 'string'
      ? value
      : JSON.stringify(value, null, 2);

/** The diff rows for the custom fields that differ */
const fieldDiffs = (
  before: Record<string, unknown>,
  after: Record<string, unknown>
): Array<{ name: string; lines: DiffLine[] }> =>
  Array.from(new Set([...Object.keys(before), ...Object.keys(after)]))
    .sort()
    .map((name) => ({ name, lines: diffLines(asText(before[name]), asText(after[name])) }))
    .filter(({ lines }) => hasChanges(lines));

const DiffView: React.FC<{ lines: DiffLine[] }> = ({ lines }) => (
  <pre className={styles.diff}>
    {lines.map((line, index) => (
      <div
        key={index}
        className={cx(
          styles.line,
          line.kind === 'add' && styles.add,
          line.kind === 'remove' && styles.remove
        )}
        data-kind={line.kind}
      >
        <span className={styles.sign} aria-hidden="true">
          {line.kind === 'add' ? '+' : line.kind === 'remove' ? '−' : ' '}
        </span>
        <span className={styles.lineText}>{line.text || ' '}</span>
      </div>
    ))}
  </pre>
);

/**
 * One saved version, read-only: the rendered Preview of its content, and the diff from that
 * version to what the editor holds now (red: only in the version, green: only in the editor).
 */
export const ContentVersionPreviewModal: React.FC<ContentVersionPreviewModalProps> = ({
  contentType,
  version,
  current,
  memberLevels = [],
  canRestore = true,
  onRestore,
  onClose,
}) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('preview');
  const idPrefix = useId();
  const fields = useMemo(() => version.fields ?? {}, [version.fields]);

  const sections = useMemo(() => {
    // Compare only the fields the editor knows
    const known = Object.fromEntries(Object.entries(fields).filter(([k]) => k in current.fields));
    return [
      {
        id: 'title',
        label: t('editor.diff.title'),
        lines: diffLines(version.title, current.title),
      },
      { id: 'slug', label: t('editor.diff.slug'), lines: diffLines(version.slug, current.slug) },
      ...fieldDiffs(known, current.fields).map(({ name, lines }) => ({
        id: `field-${name}`,
        label: name,
        lines,
      })),
      {
        id: 'body',
        label: t('editor.diff.body'),
        lines: diffLines(blocksToMarkdown(version.blocks), blocksToMarkdown(current.blocks)),
      },
    ].filter((section) => hasChanges(section.lines));
  }, [version, current, fields, t]);

  const category = typeof fields.category_id === 'string' ? fields.category_id : '';
  const layout = typeof fields.layout === 'string' ? fields.layout : undefined;
  const access = typeof fields.access === 'string' ? fields.access : '';

  return (
    <Modal
      title={t('editor.versionPreviewTitle', { revision: version.revision })}
      icon={<History size={18} />}
      onClose={onClose}
      wide
      actions={
        canRestore ? (
          <Button
            variant="tool"
            aria-label={t('editor.restoreVersionAria', { revision: version.revision })}
            onClick={() => onRestore(version.revision)}
          >
            <RotateCcw size={14} />
            <span>{t('editor.restoreVersion')}</span>
          </Button>
        ) : undefined
      }
    >
      <div className={styles.container}>
        <p className={styles.meta}>
          <time dateTime={version.created_at}>{formatDateTime(version.created_at)}</time>
          {version.created_by && <span>{version.created_by}</span>}
          {version.change_note && <span>{version.change_note}</span>}
        </p>

        <div className={styles.tabs} role="tablist" aria-label={t('editor.versionPreviewAria')}>
          {(['preview', 'diff'] as const).map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              id={`${idPrefix}-tab-${id}`}
              aria-selected={tab === id}
              aria-controls={`${idPrefix}-panel`}
              className={cx(styles.tab, tab === id && styles.activeTab)}
              onClick={() => setTab(id)}
            >
              {t(`editor.versionTabs.${id}`)}
            </button>
          ))}
        </div>

        <div
          id={`${idPrefix}-panel`}
          role="tabpanel"
          aria-labelledby={`${idPrefix}-tab-${tab}`}
          className={styles.panel}
        >
          {tab === 'preview' ? (
            <Preview
              fluid
              contentType={contentType}
              title={version.title}
              slug={version.slug}
              categoryId={category}
              layout={layout}
              access={access}
              memberLevels={memberLevels}
              blocks={version.blocks}
            />
          ) : (
            <div className={styles.diffPanel}>
              <p className={styles.legend}>{t('editor.diff.legend')}</p>
              {sections.length === 0 ? (
                <p className={styles.noDiff} role="status">
                  {t('editor.diff.none')}
                </p>
              ) : (
                sections.map((section) => (
                  <section key={section.id} className={styles.section}>
                    <h3>{section.label}</h3>
                    <DiffView lines={section.lines} />
                  </section>
                ))
              )}
            </div>
          )}
        </div>
      </div>
    </Modal>
  );
};
