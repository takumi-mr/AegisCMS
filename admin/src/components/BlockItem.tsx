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
} from 'lucide-react';
import type { BlockNode } from '../types';
import { cx } from '../utils/cx';
import { altFromKey } from '../utils/media';
import { ImageUploader } from './ImageUploader';
import { MediaLibrary } from './MediaLibrary';
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
}) => {
  const [isLibraryOpen, setIsLibraryOpen] = useState<boolean>(false);

  const getIcon = () => {
    switch (block.name) {
      case 'core/heading':
        return <Hash size={14} />;
      case 'core/code':
        return <Code2 size={14} />;
      case 'core/image':
        return <ImageIcon size={14} />;
      default:
        return <AlignLeft size={14} />;
    }
  };

  return (
    <div className={styles.card}>
      <div className={styles.header}>
        <div className={styles.badge}>
          {getIcon()}
          <span>{block.name}</span>
        </div>

        <div className={styles.controls}>
          <Button variant="icon" onClick={onMoveUp} disabled={index === 0} title="Move Up">
            <ArrowUp size={14} />
          </Button>
          <Button
            variant="icon"
            onClick={onMoveDown}
            disabled={index === total - 1}
            title="Move Down"
          >
            <ArrowDown size={14} />
          </Button>
          <Button variant="icon" danger onClick={onDelete} title="Delete Block">
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
              placeholder="Heading text..."
              value={block.text || ''}
              onChange={(e) => onChange({ ...block, text: e.target.value })}
            />
          </div>
        </div>
      )}

      {block.name === 'core/paragraph' && (
        <textarea
          className={styles.textarea}
          placeholder="Start typing your paragraph content..."
          value={block.text || ''}
          onChange={(e) => onChange({ ...block, text: e.target.value })}
        />
      )}

      {block.name === 'core/code' && (
        <div className={styles.stack}>
          <div className={styles.inline}>
            <span className={styles.languageLabel}>Language:</span>
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
            placeholder="// Enter source code here..."
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
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
            <span>Choose from media library</span>
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
            placeholder="Image URL (e.g. https://images.unsplash.com/...)"
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
            placeholder="Alt description text..."
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
              alt={block.attributes.alt || 'Preview'}
              className={styles.imagePreview}
              onError={(e) => {
                (e.target as HTMLElement).style.display = 'none';
              }}
            />
          )}
        </div>
      )}
    </div>
  );
};
