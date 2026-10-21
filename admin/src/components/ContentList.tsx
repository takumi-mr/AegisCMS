import React from 'react';
import { Plus, FileText } from 'lucide-react';
import type { ContentItem } from '../types';

interface ContentListProps {
  contents: ContentItem[];
  selectedId: string | null;
  onSelect: (content: ContentItem) => void;
  onNew: () => void;
  contentType: string;
}

export const ContentList: React.FC<ContentListProps> = ({
  contents,
  selectedId,
  onSelect,
  onNew,
  contentType,
}) => {
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">{contentType}s ({contents.length})</span>
        <button className="tool-btn" onClick={onNew} title="Create new content">
          <Plus size={14} />
          <span>New</span>
        </button>
      </div>

      <div className="content-list">
        {contents.map((item) => (
          <div
            key={item.id}
            className={`content-card ${selectedId === item.id ? 'active' : ''}`}
            onClick={() => onSelect(item)}
          >
            <div className="content-card-title">{item.title || 'Untitled'}</div>
            <div className="content-card-meta">
              <span>/{item.slug}</span>
              <span className={`badge-status ${item.status}`}>{item.status}</span>
            </div>
          </div>
        ))}

        {contents.length === 0 && (
          <div style={{ textAlign: 'center', padding: '32px 16px', color: 'var(--text-muted)' }}>
            <FileText size={32} style={{ margin: '0 auto 8px', opacity: 0.5 }} />
            <p style={{ fontSize: '0.85rem' }}>No articles yet.<br />Click "+ New" to create one.</p>
          </div>
        )}
      </div>
    </aside>
  );
};
