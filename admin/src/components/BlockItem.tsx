import React, { useState } from 'react';
import {
  ArrowUp,
  ArrowDown,
  Trash2,
  Hash,
  AlignLeft,
  Code2,
  Image as ImageIcon,
  Images,
  List,
  FileCode,
  Table,
  ListTree,
  Quote,
  Minus,
  Info,
  Puzzle,
  ListCollapse,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { BlockNode, PluginBlockDefinition } from '../types';
import { cx } from '../utils/cx';
import { altFromKey } from '../utils/media';
import { CALLOUT_VARIANTS, TOC_DEFAULT_MAX_LEVEL } from '../utils/markdown';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
import { TableBlockEditor } from './TableBlockEditor';
import { PluginBlockEditor } from './PluginBlockEditor';
import { isPluginBlockName, localize } from '../utils/pluginBlocks';
import { Button } from './ui/Button';
import styles from './BlockItem.module.css';

interface BlockItemProps {
  block: BlockNode;
  index: number;
  total: number;
  onChange: (updated: BlockNode) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onDelete: () => void;
  /** Uploading needs the editor role; viewers can still pick from the media library */
  canUpload: boolean;
  /** Custom blocks of the enabled plugins (`GET /api/v1/blocks`) */
  blockDefinitions?: PluginBlockDefinition[];
}

export const BlockItem: React.FC<BlockItemProps> = ({
  block,
  index,
  total,
  onChange,
  onMoveUp,
  onMoveDown,
  onDelete,
  canUpload,
  blockDefinitions = [],
}) => {
  const { t, i18n } = useTranslation();
  const pluginBlock = isPluginBlockName(block.name);
  const definition = pluginBlock ? blockDefinitions.find((d) => d.name === block.name) : undefined;
  const [isLibraryOpen, setIsLibraryOpen] = useState<boolean>(false);

  const getIcon = () => {
    switch (block.name) {
      case 'core/heading':
        return <Hash size={14} />;
      case 'core/code':
        return <Code2 size={14} />;
      case 'core/html':
        return <FileCode size={14} />;
      case 'core/image':
        return <ImageIcon size={14} />;
      case 'core/list':
        return <List size={14} />;
      case 'core/table':
        return <Table size={14} />;
      case 'core/toc':
        return <ListTree size={14} />;
      case 'core/quote':
        return <Quote size={14} />;
      case 'core/separator':
        return <Minus size={14} />;
      case 'core/callout':
        return <Info size={14} />;
      case 'core/details':
        return <ListCollapse size={14} />;
      default:
        return pluginBlock ? <Puzzle size={14} /> : <AlignLeft size={14} />;
    }
  };

  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <div className={styles.badge}>
          {getIcon()}
          <span>
            {definition ? `${localize(definition.label, i18n.language)} · ` : ''}
            {block.name}
          </span>
        </div>

        <div className={styles.controls}>
          <Button
            variant="icon"
            onClick={onMoveUp}
            disabled={index === 0}
            title={t('blocks.moveUp')}
          >
            <ArrowUp size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={onMoveDown}
            disabled={index === total - 1}
            title={t('blocks.moveDown')}
          >
            <ArrowDown size={14} />
          </Button>
          <Button variant="icon" danger onClick={onDelete} title={t('blocks.delete')}>
            <Trash2 size={14} />
          </Button>
        </div>
      </div>

      {/* Block Content Editor */}
      {block.name === 'core/heading' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <select
              className={styles.levelSelect}
              value={block.attributes?.level || 2}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, level: Number(e.target.value) },
                })
              }
            >
              {[1, 2, 3, 4, 5, 6].map((l) => (
                <option key={l} value={l}>
                  H{l}
                </option>
              ))}
            </select>
            <input
              type="text"
              className={styles.headingInput}
              placeholder={t('blocks.headingPlaceholder')}
              value={block.text || ''}
              onChange={(e) => onChange({ ...block, text: e.target.value })}
            />
          </div>
        </div>
      )}

      {block.name === 'core/paragraph' && (
        <textarea
          className={styles.textarea}
          placeholder={t('blocks.paragraphPlaceholder')}
          value={block.text || ''}
          onChange={(e) => onChange({ ...block, text: e.target.value })}
        />
      )}

      {block.name === 'core/code' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <span className={styles.languageLabel}>{t('blocks.language')}</span>
            <select
              className={styles.languageSelect}
              value={block.attributes?.language || 'rust'}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, language: e.target.value },
                })
              }
            >
              <option value="rust">Rust</option>
              <option value="typescript">TypeScript</option>
              <option value="json">JSON</option>
              <option value="bash">Bash / Shell</option>
              <option value="python">Python</option>
              <option value="html">HTML</option>
              <option value="css">CSS</option>
            </select>
          </div>
          <textarea
            className={cx(styles.textarea, styles.code)}
            placeholder={t('blocks.codePlaceholder')}
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
          />
        </div>
      )}

      {block.name === 'core/html' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <span className={styles.languageLabel}>{t('blocks.htmlBlock')}</span>
            <select
              className={styles.languageSelect}
              aria-label={t('blocks.htmlMode')}
              value={block.attributes?.mode === 'sandbox' ? 'sandbox' : 'sanitized'}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, mode: e.target.value },
                })
              }
            >
              <option value="sanitized">{t('blocks.htmlModeSanitized')}</option>
              <option value="sandbox">{t('blocks.htmlModeSandbox')}</option>
            </select>
          </div>
          <textarea
            className={cx(styles.textarea, styles.code)}
            placeholder={t('blocks.htmlPlaceholder')}
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
            rows={8}
            spellCheck={false}
          />
        </div>
      )}

      {block.name === 'core/image' && (
        <div className={styles.stack}>
          {canUpload && (
            <ImageUploader
              onUploaded={(meta, file) =>
                onChange({
                  ...block,
                  attributes: {
                    ...block.attributes,
                    src: meta.url,
                    // Keep an existing alt text; otherwise derive one from the file name
                    alt: block.attributes?.alt || file.name.replace(/\.[^.]+$/, ''),
                  },
                })
              }
            />
          )}
          <Button className={styles.libraryButton} onClick={() => setIsLibraryOpen(true)}>
            <Images size={14} />
            <span>{t('blocks.library')}</span>
          </Button>
          {isLibraryOpen && (
            <MediaLibrary
              canDelete={canUpload}
              onClose={() => setIsLibraryOpen(false)}
              onSelect={(object) => {
                setIsLibraryOpen(false);
                onChange({
                  ...block,
                  attributes: {
                    ...block.attributes,
                    src: object.url,
                    alt: block.attributes?.alt || altFromKey(object.key),
                  },
                });
              }}
            />
          )}
          <input
            type="text"
            className={styles.urlInput}
            placeholder={t('blocks.urlPlaceholder')}
            value={block.attributes?.src || ''}
            onChange={(e) =>
              onChange({
                ...block,
                attributes: { ...block.attributes, src: e.target.value },
              })
            }
          />
          <input
            type="text"
            className={styles.altInput}
            placeholder={t('blocks.altPlaceholder')}
            value={block.attributes?.alt || ''}
            onChange={(e) =>
              onChange({
                ...block,
                attributes: { ...block.attributes, alt: e.target.value },
              })
            }
          />
          {block.attributes?.src && (
            <img
              // Re-mount when the URL changes so an earlier load error does not keep it hidden
              key={block.attributes.src}
              src={block.attributes.src}
              alt={block.attributes.alt || t('blocks.previewAlt')}
              className={styles.imagePreview}
              onError={(e) => {
                (e.target as HTMLElement).style.display = 'none';
              }}
            />
          )}
        </div>
      )}

      {block.name === 'core/list' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <span className={styles.languageLabel}>{t('blocks.listType')}</span>
            <select
              className={styles.levelSelect}
              value={block.attributes?.ordered ? 'ordered' : 'unordered'}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, ordered: e.target.value === 'ordered' },
                })
              }
            >
              <option value="unordered">{t('blocks.bullet')}</option>
              <option value="ordered">{t('blocks.numbered')}</option>
            </select>
          </div>
          <textarea
            className={styles.textarea}
            placeholder={t('blocks.listPlaceholder')}
            value={(block.children || []).map((c) => c.text || '').join('\n')}
            onChange={(e) => {
              const lines = e.target.value.split('\n');
              onChange({
                ...block,
                children: lines.map((line) => ({
                  name: 'core/list-item',
                  attributes: {},
                  text: line,
                })),
              });
            }}
          />
        </div>
      )}
      {block.name === 'core/table' && <TableBlockEditor block={block} onChange={onChange} />}

      {definition && (
        <PluginBlockEditor
          block={block}
          definition={definition}
          onChange={onChange}
          canUpload={canUpload}
        />
      )}

      {pluginBlock && !definition && (
        <p className={styles.note}>
          {t('blocks.plugin.unavailable', { plugin: block.name.split('/')[0] })}
        </p>
      )}

      {block.name === 'core/toc' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <input
              type="text"
              className={styles.urlInput}
              aria-label={t('blocks.tocTitle')}
              placeholder={t('blocks.tocTitle')}
              value={block.attributes?.title || ''}
              onChange={(e) =>
                onChange({ ...block, attributes: { ...block.attributes, title: e.target.value } })
              }
            />
            <span className={styles.languageLabel}>{t('blocks.tocDepth')}</span>
            <select
              className={styles.levelSelect}
              aria-label={t('blocks.tocDepth')}
              value={block.attributes?.maxLevel || TOC_DEFAULT_MAX_LEVEL}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, maxLevel: Number(e.target.value) },
                })
              }
            >
              {[2, 3, 4].map((l) => (
                <option key={l} value={l}>
                  {l === 2 ? 'H2' : `H2–H${l}`}
                </option>
              ))}
            </select>
          </div>
          <p className={styles.note}>{t('blocks.tocNote')}</p>
        </div>
      )}

      {block.name === 'core/quote' && (
        <div className={styles.stack}>
          <textarea
            className={cx(styles.textarea, styles.quote)}
            placeholder={t('blocks.quotePlaceholder')}
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
          />
          <input
            type="text"
            className={styles.altInput}
            placeholder={t('blocks.citePlaceholder')}
            value={block.attributes?.cite || ''}
            onChange={(e) =>
              onChange({ ...block, attributes: { ...block.attributes, cite: e.target.value } })
            }
          />
        </div>
      )}

      {block.name === 'core/separator' && <hr className={styles.separator} />}

      {block.name === 'core/details' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <input
              type="text"
              className={cx(styles.urlInput, styles.grow)}
              aria-label={t('blocks.detailsSummary')}
              placeholder={t('blocks.detailsSummary')}
              value={block.attributes?.summary || ''}
              onChange={(e) =>
                onChange({ ...block, attributes: { ...block.attributes, summary: e.target.value } })
              }
            />
            <label className={styles.languageLabel}>
              <input
                type="checkbox"
                checked={block.attributes?.open === true}
                onChange={(e) =>
                  onChange({
                    ...block,
                    attributes: { ...block.attributes, open: e.target.checked },
                  })
                }
              />{' '}
              {t('blocks.detailsOpen')}
            </label>
          </div>
          <textarea
            className={styles.textarea}
            placeholder={t('blocks.detailsPlaceholder')}
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
          />
        </div>
      )}

      {block.name === 'core/callout' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <select
              className={styles.levelSelect}
              aria-label={t('blocks.calloutVariant')}
              value={
                CALLOUT_VARIANTS.includes(block.attributes?.variant)
                  ? block.attributes.variant
                  : 'note'
              }
              onChange={(e) =>
                onChange({ ...block, attributes: { ...block.attributes, variant: e.target.value } })
              }
            >
              {CALLOUT_VARIANTS.map((v) => (
                <option key={v} value={v}>
                  {t(`blocks.callout.${v}`)}
                </option>
              ))}
            </select>
            <input
              type="text"
              className={cx(styles.urlInput, styles.grow)}
              aria-label={t('blocks.calloutTitle')}
              placeholder={t('blocks.calloutTitle')}
              value={block.attributes?.title || ''}
              onChange={(e) =>
                onChange({ ...block, attributes: { ...block.attributes, title: e.target.value } })
              }
            />
          </div>
          <textarea
            className={styles.textarea}
            placeholder={t('blocks.calloutPlaceholder')}
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
          />
        </div>
      )}
    </div>
  );
};
