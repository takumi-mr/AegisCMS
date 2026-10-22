import React, { useState, useEffect } from 'react';
import {
  Plus,
  Hash,
  AlignLeft,
  Code2,
  Image as ImageIcon,
  CheckCircle2,
  AlertCircle,
  Tags,
} from 'lucide-react';
import { Header, type AppView } from './components/Header';
import { AuditLogView } from './components/AuditLogView';
import { ThemeManager } from './components/ThemeManager';
import { LoginView, SessionCheckingScreen } from './components/LoginView';
import { CategoryManager } from './components/CategoryManager';
import { ContentList } from './components/ContentList';
import { BlockItem } from './components/BlockItem';
import { Preview } from './components/Preview';
import { aegisApi, setUnauthorizedHandler } from './api/client';
import { setSessionToken } from './api/session';
import type { BlockNode, ContentItem, SessionInfo } from './types';
import { categoryIdOf } from './utils/category';
import { canEdit, isAdmin } from './utils/roles';
import { cx } from './utils/cx';
import { Button } from './components/ui/Button';
import { Field, FieldRow } from './components/ui/Field';
import styles from './App.module.css';

export const App: React.FC = () => {
  // undefined = still checking the stored token, null = logged out
  const [session, setSession] = useState<SessionInfo | null | undefined>(undefined);
  const [loginNotice, setLoginNotice] = useState<string | null>(null);
  const [view, setView] = useState<AppView>('editor');
  const [isOnline, setIsOnline] = useState<boolean>(false);
  const [contentType] = useState<string>('article');
  const [contents, setContents] = useState<ContentItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [categories, setCategories] = useState<ContentItem[]>([]);
  const [categoryId, setCategoryId] = useState<string>('');
  const [isCategoryManagerOpen, setIsCategoryManagerOpen] = useState<boolean>(false);

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

  const loadCategories = async () => {
    try {
      setCategories(await aegisApi.getContents('category'));
    } catch (err) {
      console.warn('Could not load categories:', err);
    }
  };

  const loadContents = async () => {
    try {
      const list = await aegisApi.getContents(contentType);
      setContents(list);
    } catch (err) {
      console.warn('Could not load contents:', err);
    }
  };

  // 1. Restore the session of the stored token; any later 401 returns to the login screen
  useEffect(() => {
    aegisApi
      .getCurrentSession()
      .then((current) => {
        if (!current) setSessionToken('');
        setSession(current);
      })
      .catch(() => {
        // Server unreachable: keep the token and ask to sign in once it is back
        setLoginNotice('Cannot reach the AegisCMS server. Check that it is running.');
        setSession(null);
      });

    setUnauthorizedHandler(() => {
      setSessionToken('');
      setSession(null);
      setLoginNotice('Your session has expired. Sign in again.');
    });
    return () => setUnauthorizedHandler(null);
  }, []);

  // 2. Server health indicator
  useEffect(() => {
    const checkServer = async () => {
      try {
        await aegisApi.checkHealth();
        setIsOnline(true);
      } catch {
        setIsOnline(false);
      }
    };
    checkServer();
    const timer = setInterval(checkServer, 15000);
    return () => clearInterval(timer);
  }, []);

  // 3. Load data whenever a user logs in
  const sessionId = session?.id;
  useEffect(() => {
    if (!sessionId) return;
    loadContents();
    loadCategories();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  const handleLoggedIn = (loggedIn: SessionInfo) => {
    setSessionToken(loggedIn.id);
    setLoginNotice(null);
    setSession(loggedIn);
  };

  const handleLogout = async () => {
    try {
      await aegisApi.logout();
    } catch (err) {
      console.warn('Logout request failed:', err);
    }
    setSessionToken('');
    setSession(null);
    setLoginNotice('You have signed out.');
    setView('editor');
    setContents([]);
    setCategories([]);
  };

  const handleCategoryCreated = (category: ContentItem) => {
    setCategories((prev) => [category, ...prev.filter((c) => c.id !== category.id)]);
    setCategoryId(category.id);
    showToast(`Category "${category.title}" created`);
  };

  const handleSelectContent = (item: ContentItem) => {
    setSelectedId(item.id);
    setTitle(item.title);
    setSlug(item.slug);
    setStatus(item.status === 'published' ? 'published' : 'draft');
    setBlocks(item.blocks || []);
    setCategoryId(categoryIdOf(item) ?? '');
  };

  const handleNew = () => {
    setSelectedId(null);
    setTitle('Untitled Document');
    setSlug(`untitled-${Date.now().toString(36)}`);
    setStatus('draft');
    setCategoryId('');
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
        // author_id is omitted so the server records the logged-in user
        // Omit the relation entirely when no category is selected
        ...(categoryId ? { category_id: categoryId } : {}),
      });

      showToast(`Saved! Wasm plugin hooks executed successfully (ID: ${saved.id.slice(0, 8)})`);
      loadContents();
      // Show what was stored: plugins may change the title, slug and blocks on save
      handleSelectContent(saved);
    } catch (err: any) {
      showToast(err.message || 'Failed to save', 'error');
    } finally {
      setIsSaving(false);
    }
  };

  if (session === undefined) {
    return <SessionCheckingScreen />;
  }
  if (session === null) {
    return <LoginView onLoggedIn={handleLoggedIn} notice={loginNotice} />;
  }

  const editable = canEdit(session);

  return (
    <div className={styles.app}>
      <Header
        view={view}
        onViewChange={setView}
        isOnline={isOnline}
        onSave={handleSave}
        isSaving={isSaving}
        session={session}
        canSave={editable}
        canViewAudit={isAdmin(session)}
        onLogout={handleLogout}
      />

      {view === 'audit' && isAdmin(session) ? (
        <AuditLogView />
      ) : view === 'themes' ? (
        <ThemeManager session={session} onShowToast={showToast} />
      ) : (
        <div className={styles.workspace}>
          {/* Sidebar */}
          <ContentList
            contents={contents}
            categories={categories}
            selectedId={selectedId}
            onSelect={handleSelectContent}
            onNew={handleNew}
            contentType={contentType}
          />

          {/* Main Editor Center Column */}
          <main className={styles.editorMain}>
            <div className={styles.editorColumn}>
              {/* Meta Section */}
              <div className={styles.metaSection}>
                <input
                  type="text"
                  className={styles.titleInput}
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

                <FieldRow>
                  <Field label="slug:">
                    <input
                      type="text"
                      value={slug}
                      onChange={(e) => setSlug(e.target.value)}
                      placeholder="post-slug"
                    />
                  </Field>

                  <Field label="status:" className={styles.statusField}>
                    <select value={status} onChange={(e) => setStatus(e.target.value as any)}>
                      <option value="draft">Draft</option>
                      <option value="published">Published</option>
                    </select>
                  </Field>
                </FieldRow>

                <FieldRow>
                  <Field label="category:">
                    <select
                      value={categoryId}
                      onChange={(e) => setCategoryId(e.target.value)}
                      aria-label="Category"
                    >
                      <option value="">No category</option>
                      {categories.map((category) => (
                        <option key={category.id} value={category.id}>
                          {category.title}
                        </option>
                      ))}
                      {categoryId && !categories.some((c) => c.id === categoryId) && (
                        <option value={categoryId}>
                          Unknown category ({categoryId.slice(0, 8)})
                        </option>
                      )}
                    </select>
                  </Field>
                  <Button
                    variant="tool"
                    onClick={() => setIsCategoryManagerOpen(true)}
                    disabled={!editable}
                    title={editable ? 'List and create categories' : 'Requires the editor role'}
                  >
                    <Tags size={14} />
                    <span>Manage</span>
                  </Button>
                </FieldRow>
              </div>

              {/* Block Insertion Toolbar */}
              <div className={styles.blockToolbar}>
                <span className={styles.toolbarLabel}>Add Block</span>
                <Button
                  variant="tool"
                  onClick={() => addBlock('core/heading', 'Section Title', { level: 2 })}
                >
                  <Hash size={14} />
                  <span>Heading</span>
                </Button>
                <Button variant="tool" onClick={() => addBlock('core/paragraph', '')}>
                  <AlignLeft size={14} />
                  <span>Paragraph</span>
                </Button>
                <Button
                  variant="tool"
                  onClick={() => addBlock('core/code', '', { language: 'rust' })}
                >
                  <Code2 size={14} />
                  <span>Code Block</span>
                </Button>
                <Button
                  variant="tool"
                  onClick={() =>
                    // Starts empty: the image and its alt text come from an upload or the media library
                    addBlock('core/image', '', { src: '', alt: '' })
                  }
                >
                  <ImageIcon size={14} />
                  <span>Image</span>
                </Button>
              </div>

              {/* Block Canvas */}
              <div className={styles.blocksList}>
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
                    canUpload={editable}
                  />
                ))}

                {blocks.length === 0 && (
                  <div className={styles.emptyBlocks}>
                    <p>This post has no content blocks.</p>
                    <Button className={styles.addFirst} onClick={() => addBlock('core/paragraph')}>
                      <Plus size={16} /> Add First Paragraph
                    </Button>
                  </div>
                )}
              </div>
            </div>

            {/* Right Live Preview Column */}
            <Preview
              contentType={contentType}
              title={title}
              slug={slug}
              categoryId={categoryId}
              blocks={blocks}
            />
          </main>
        </div>
      )}

      {isCategoryManagerOpen && (
        <CategoryManager
          categories={categories}
          onCreated={handleCategoryCreated}
          onClose={() => setIsCategoryManagerOpen(false)}
        />
      )}

      {/* Toast Feedback */}
      {toast && (
        <div className={styles.toastContainer}>
          <div className={cx(styles.toast, styles[toast.type])}>
            {toast.type === 'success' ? <CheckCircle2 size={18} /> : <AlertCircle size={18} />}
            <span>{toast.message}</span>
          </div>
        </div>
      )}
    </div>
  );
};
export default App;
