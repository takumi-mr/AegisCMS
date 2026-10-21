import React, { useState, useEffect } from 'react';
import { Plus, Hash, AlignLeft, Code2, Image as ImageIcon, CheckCircle2, AlertCircle } from 'lucide-react';
import { Header } from './components/Header';
import { ContentList } from './components/ContentList';
import { BlockItem } from './components/BlockItem';
import { Preview } from './components/Preview';
import { aegisApi } from './api/client';
import type { BlockNode, ContentItem } from './types';

export const App: React.FC = () => {
  const [isOnline, setIsOnline] = useState<boolean>(false);
  const [contentType] = useState<string>('article');
  const [contents, setContents] = useState<ContentItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Editor Form State
  const [title, setTitle] = useState<string>('Next-Gen WebAssembly CMS with Aegis');
  const [slug, setSlug] = useState<string>('next-gen-wasm-cms');
  const [status, setStatus] = useState<'draft' | 'published'>('published');
  const [blocks, setBlocks] = useState<BlockNode[]>([
    {
      name: 'core/heading',
      attributes: { level: 2 },
      text: 'Deterministic Content Architecture',
    },
    {
      name: 'core/paragraph',
      attributes: {},
      text: 'AegisCMS replaces monolithic PHP plugin runtimes with memory-safe WebAssembly sandboxes governed by strict capability-based manifests.',
    },
    {
      name: 'core/code',
      attributes: { language: 'rust' },
      text: '// Zero-Trust capability validation in Rust\nfn verify_domain(allowed: &[String], url: &str) -> bool {\n    allowed.iter().any(|d| url.contains(d))\n}',
    },
  ]);

  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [toast, setToast] = useState<{ message: string; type: 'success' | 'error' } | null>(null);

  const showToast = (message: string, type: 'success' | 'error' = 'success') => {
    setToast({ message, type });
    setTimeout(() => setToast(null), 4000);
  };

  // 1. Initial health and data loading
  useEffect(() => {
    const checkServer = async () => {
      try {
        await aegisApi.checkHealth();
        setIsOnline(true);
        loadContents();
      } catch (e) {
        setIsOnline(false);
      }
    };
    checkServer();
    const timer = setInterval(checkServer, 15000);
    return () => clearInterval(timer);
  }, []);

  const loadContents = async () => {
    try {
      const list = await aegisApi.getContents(contentType);
      setContents(list);
    } catch (err) {
      console.warn('Could not load contents:', err);
    }
  };

  const handleSelectContent = (item: ContentItem) => {
    setSelectedId(item.id);
    setTitle(item.title);
    setSlug(item.slug);
    setStatus(item.status === 'published' ? 'published' : 'draft');
    setBlocks(item.blocks || []);
  };

  const handleNew = () => {
    setSelectedId(null);
    setTitle('Untitled Document');
    setSlug(`untitled-${Date.now().toString(36)}`);
    setStatus('draft');
    setBlocks([
      { name: 'core/heading', attributes: { level: 2 }, text: 'New Heading' },
      { name: 'core/paragraph', attributes: {}, text: '' },
    ]);
  };

  // Block Manipulation
  const addBlock = (name: string, defaultText = '', defaultAttrs: Record<string, any> = {}) => {
    setBlocks([...blocks, { name, text: defaultText, attributes: defaultAttrs }]);
  };

  const updateBlock = (index: number, updated: BlockNode) => {
    const next = [...blocks];
    next[index] = updated;
    setBlocks(next);
  };

  const moveBlock = (index: number, direction: 'up' | 'down') => {
    const next = [...blocks];
    const target = direction === 'up' ? index - 1 : index + 1;
    if (target < 0 || target >= next.length) return;
    const [removed] = next.splice(index, 1);
    next.splice(target, 0, removed);
    setBlocks(next);
  };

  const deleteBlock = (index: number) => {
    setBlocks(blocks.filter((_, i) => i !== index));
  };

  // Save via API Gateway & Wasm Hooks
  const handleSave = async () => {
    if (!title.trim()) {
      showToast('Title is required', 'error');
      return;
    }
    if (!slug.trim()) {
      showToast('Slug is required', 'error');
      return;
    }

    setIsSaving(true);
    try {
      const saved = await aegisApi.createContent(contentType, {
        title,
        slug,
        status,
        blocks,
        author_id: 'usr_takumi',
      });

      showToast(`Saved! Wasm plugin hooks executed successfully (ID: ${saved.id.slice(0, 8)})`);
      loadContents();
      setSelectedId(saved.id);
    } catch (err: any) {
      showToast(err.message || 'Failed to save', 'error');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="app-container">
      <Header
        isOnline={isOnline}
        onSave={handleSave}
        isSaving={isSaving}
      />

      <div className="workspace-body">
        {/* Sidebar */}
        <ContentList
          contents={contents}
          selectedId={selectedId}
          onSelect={handleSelectContent}
          onNew={handleNew}
          contentType={contentType}
        />

        {/* Main Editor Center Column */}
        <main className="editor-main">
          <div className="editor-column">
            {/* Meta Section */}
            <div className="meta-section">
              <input
                type="text"
                className="title-input"
                placeholder="Enter post title..."
                value={title}
                onChange={(e) => {
                  setTitle(e.target.value);
                  if (!selectedId) {
                    setSlug(
                      e.target.value
                        .toLowerCase()
                        .replace(/[^a-z0-9-]+/g, '-')
                        .replace(/^-|-$/g, '')
                    );
                  }
                }}
              />

              <div className="meta-row">
                <div className="input-field">
                  <span className="input-prefix">slug:</span>
                  <input
                    type="text"
                    value={slug}
                    onChange={(e) => setSlug(e.target.value)}
                    placeholder="post-slug"
                  />
                </div>

                <div className="input-field" style={{ maxWidth: '180px' }}>
                  <span className="input-prefix">status:</span>
                  <select
                    value={status}
                    onChange={(e) => setStatus(e.target.value as any)}
                  >
                    <option value="draft">Draft</option>
                    <option value="published">Published</option>
                  </select>
                </div>
              </div>
            </div>

            {/* Block Insertion Toolbar */}
            <div className="block-toolbar">
              <span className="toolbar-label">Add Block</span>
              <button
                className="tool-btn"
                onClick={() => addBlock('core/heading', 'Section Title', { level: 2 })}
              >
                <Hash size={14} />
                <span>Heading</span>
              </button>
              <button
                className="tool-btn"
                onClick={() => addBlock('core/paragraph', '')}
              >
                <AlignLeft size={14} />
                <span>Paragraph</span>
              </button>
              <button
                className="tool-btn"
                onClick={() => addBlock('core/code', '', { language: 'rust' })}
              >
                <Code2 size={14} />
                <span>Code Block</span>
              </button>
              <button
                className="tool-btn"
                onClick={() =>
                  addBlock('core/image', '', {
                    src: 'https://images.unsplash.com/photo-1618005182384-a83a8bd57fbe?w=800',
                    alt: 'Abstract gradient art',
                  })
                }
              >
                <ImageIcon size={14} />
                <span>Image</span>
              </button>
            </div>

            {/* Block Canvas */}
            <div className="blocks-list">
              {blocks.map((block, idx) => (
                <BlockItem
                  key={idx}
                  block={block}
                  index={idx}
                  total={blocks.length}
                  onChange={(updated) => updateBlock(idx, updated)}
                  onMoveUp={() => moveBlock(idx, 'up')}
                  onMoveDown={() => moveBlock(idx, 'down')}
                  onDelete={() => deleteBlock(idx)}
                />
              ))}

              {blocks.length === 0 && (
                <div style={{ textAlign: 'center', padding: '40px', color: 'var(--text-muted)' }}>
                  <p>This post has no content blocks.</p>
                  <button
                    className="btn btn-secondary"
                    style={{ marginTop: '12px' }}
                    onClick={() => addBlock('core/paragraph')}
                  >
                    <Plus size={16} /> Add First Paragraph
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* Right Live Preview Column */}
          <Preview title={title} blocks={blocks} />
        </main>
      </div>

      {/* Toast Feedback */}
      {toast && (
        <div className="toast-container">
          <div className={`toast ${toast.type}`}>
            {toast.type === 'success' ? (
              <CheckCircle2 size={18} />
            ) : (
              <AlertCircle size={18} />
            )}
            <span>{toast.message}</span>
          </div>
        </div>
      )}
    </div>
  );
};
export default App;
