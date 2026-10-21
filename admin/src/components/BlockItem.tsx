import React from 'react';
import { ArrowUp, ArrowDown, Trash2, Hash, AlignLeft, Code2, Image as ImageIcon } from 'lucide-react';
import type { BlockNode } from '../types';

interface BlockItemProps {
  block: BlockNode;
  index: number;
  total: number;
  onChange: (updated: BlockNode) => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onDelete: () => void;
}

export const BlockItem: React.FC<BlockItemProps> = ({
  block,
  index,
  total,
  onChange,
  onMoveUp,
  onMoveDown,
  onDelete,
}) => {
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
    <div className="block-card">
      <div className="block-header">
        <div className="block-badge">
          {getIcon()}
          <span>{block.name}</span>
        </div>

        <div className="block-controls">
          <button
            className="icon-btn"
            onClick={onMoveUp}
            disabled={index === 0}
            title="Move Up"
          >
            <ArrowUp size={14} />
          </button>
          <button
            className="icon-btn"
            onClick={onMoveDown}
            disabled={index === total - 1}
            title="Move Down"
          >
            <ArrowDown size={14} />
          </button>
          <button
            className="icon-btn danger"
            onClick={onDelete}
            title="Delete Block"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {/* Block Content Editor */}
      {block.name === 'core/heading' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
            <select
              value={block.attributes?.level || 2}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, level: Number(e.target.value) },
                })
              }
              style={{
                background: 'var(--bg-base)',
                color: 'var(--text-primary)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 'var(--radius-sm)',
                padding: '4px 8px',
                fontSize: '0.85rem',
                outline: 'none',
              }}
            >
              {[1, 2, 3, 4, 5, 6].map((l) => (
                <option key={l} value={l}>
                  H{l}
                </option>
              ))}
            </select>
            <input
              type="text"
              placeholder="Heading text..."
              value={block.text || ''}
              onChange={(e) => onChange({ ...block, text: e.target.value })}
              style={{
                flex: 1,
                background: 'transparent',
                border: 'none',
                color: '#fff',
                fontSize: '1.25rem',
                fontWeight: 600,
                outline: 'none',
              }}
            />
          </div>
        </div>
      )}

      {block.name === 'core/paragraph' && (
        <textarea
          className="block-textarea"
          placeholder="Start typing your paragraph content..."
          value={block.text || ''}
          onChange={(e) => onChange({ ...block, text: e.target.value })}
        />
      )}

      {block.name === 'core/code' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
            <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>Language:</span>
            <select
              value={block.attributes?.language || 'rust'}
              onChange={(e) =>
                onChange({
                  ...block,
                  attributes: { ...block.attributes, language: e.target.value },
                })
              }
              style={{
                background: 'var(--bg-base)',
                color: 'var(--accent-cyan)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 'var(--radius-sm)',
                padding: '3px 8px',
                fontSize: '0.8rem',
                fontFamily: 'var(--font-mono)',
                outline: 'none',
              }}
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
            className="block-textarea code"
            placeholder="// Enter source code here..."
            value={block.text || ''}
            onChange={(e) => onChange({ ...block, text: e.target.value })}
          />
        </div>
      )}

      {block.name === 'core/image' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <input
            type="text"
            placeholder="Image URL (e.g. https://images.unsplash.com/...)"
            value={block.attributes?.src || ''}
            onChange={(e) =>
              onChange({
                ...block,
                attributes: { ...block.attributes, src: e.target.value },
              })
            }
            style={{
              background: 'var(--bg-base)',
              border: '1px solid var(--border-subtle)',
              borderRadius: 'var(--radius-sm)',
              padding: '6px 10px',
              color: 'var(--text-primary)',
              fontSize: '0.85rem',
              outline: 'none',
            }}
          />
          <input
            type="text"
            placeholder="Alt description text..."
            value={block.attributes?.alt || ''}
            onChange={(e) =>
              onChange({
                ...block,
                attributes: { ...block.attributes, alt: e.target.value },
              })
            }
            style={{
              background: 'transparent',
              border: 'none',
              borderBottom: '1px solid var(--border-subtle)',
              padding: '4px 8px',
              color: 'var(--text-secondary)',
              fontSize: '0.85rem',
              outline: 'none',
            }}
          />
          {block.attributes?.src && (
            <img
              src={block.attributes.src}
              alt={block.attributes.alt || 'Preview'}
              style={{
                maxHeight: '140px',
                objectFit: 'cover',
                borderRadius: 'var(--radius-sm)',
                marginTop: '4px',
              }}
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
