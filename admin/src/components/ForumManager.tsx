import React, { useEffect, useState } from 'react';
import {
  ExternalLink,
  Eye,
  FolderPlus,
  Lock,
  MessagesSquare,
  MessageSquare,
  Pin,
  Plus,
  RefreshCw,
  Trash2,
  Unlock,
  Edit3,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type { CreateForumCategoryPayload, ForumCategory, ForumPost, ForumThread } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime } from '../utils/format';
import { Button } from './ui/Button';
import { Modal } from './ui/Modal';
import shop from './Shop.module.css';
import styles from './ForumManager.module.css';

interface ForumManagerProps {
  /** Admins may create, edit and delete categories; editors moderate threads and posts */
  isAdmin: boolean;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

type TabMode = 'threads' | 'categories';

export const ForumManager: React.FC<ForumManagerProps> = ({ isAdmin, onShowToast }) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabMode>('threads');
  const [categories, setCategories] = useState<ForumCategory[]>([]);
  const [selectedCategorySlug, setSelectedCategorySlug] = useState<string>('all');
  const [threads, setThreads] = useState<ForumThread[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // Thread posts view modal
  const [postsModalThread, setPostsModalThread] = useState<ForumThread | null>(null);
  const [threadPosts, setThreadPosts] = useState<ForumPost[]>([]);
  const [loadingPosts, setLoadingPosts] = useState<boolean>(false);

  // Category create/edit modal
  const [categoryModal, setCategoryModal] = useState<{
    mode: 'create' | 'edit';
    category?: ForumCategory;
  } | null>(null);
  const [categoryForm, setCategoryForm] = useState<CreateForumCategoryPayload>({
    slug: '',
    name: '',
    description: '',
    position: 0,
    is_private: false,
  });
  const [isSubmittingCategory, setIsSubmittingCategory] = useState<boolean>(false);

  // Load categories on mount
  const loadCategories = async () => {
    try {
      const cats = await aegisApi.listForumCategories();
      setCategories(cats);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load categories';
      setError(msg);
    }
  };

  // Load threads
  const loadThreads = async () => {
    setLoading(true);
    setError(null);
    try {
      const loaded = await aegisApi.listForumThreads({
        category_slug: selectedCategorySlug === 'all' ? undefined : selectedCategorySlug,
        limit: 50,
      });
      setThreads(loaded);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load threads';
      setError(msg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadCategories();
  }, []);

  useEffect(() => {
    if (activeTab === 'threads') {
      loadThreads();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, selectedCategorySlug]);

  // Handle Thread Pin
  const handleTogglePin = async (thread: ForumThread) => {
    const nextState = !thread.is_pinned;
    try {
      await aegisApi.pinForumThread(thread.id, nextState);
      setThreads((prev) =>
        prev.map((t) => (t.id === thread.id ? { ...t, is_pinned: nextState } : t))
      );
      onShowToast(
        nextState ? t('forum.toasts.threadPinned') : t('forum.toasts.threadUnpinned'),
        'success'
      );
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to update pin';
      onShowToast(msg, 'error');
    }
  };

  // Handle Thread Lock
  const handleToggleLock = async (thread: ForumThread) => {
    const nextState = !thread.is_locked;
    try {
      await aegisApi.lockForumThread(thread.id, nextState);
      setThreads((prev) =>
        prev.map((t) => (t.id === thread.id ? { ...t, is_locked: nextState } : t))
      );
      onShowToast(
        nextState ? t('forum.toasts.threadLocked') : t('forum.toasts.threadUnlocked'),
        'success'
      );
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to update lock';
      onShowToast(msg, 'error');
    }
  };

  // Handle Thread Delete
  const handleDeleteThread = async (thread: ForumThread) => {
    if (!window.confirm(t('forum.confirmDeleteThread', { title: thread.title }))) {
      return;
    }
    try {
      await aegisApi.deleteForumThread(thread.id);
      setThreads((prev) => prev.filter((t) => t.id !== thread.id));
      onShowToast(t('forum.toasts.threadDeleted'), 'success');
      loadCategories(); // refresh thread counts
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to delete thread';
      onShowToast(msg, 'error');
    }
  };

  // View Posts in Thread
  const handleViewPosts = async (thread: ForumThread) => {
    setPostsModalThread(thread);
    setLoadingPosts(true);
    try {
      const posts = await aegisApi.listForumPosts(thread.id, { limit: 100 });
      setThreadPosts(posts);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load posts';
      onShowToast(msg, 'error');
    } finally {
      setLoadingPosts(false);
    }
  };

  // Handle Post Delete
  const handleDeletePost = async (post: ForumPost) => {
    if (!window.confirm(t('forum.confirmDeletePost', { number: post.post_number }))) {
      return;
    }
    try {
      await aegisApi.deleteForumPost(post.id);
      setThreadPosts((prev) =>
        prev.map((p) =>
          p.id === post.id ? { ...p, is_deleted: true, body: '[Deleted by moderator]' } : p
        )
      );
      onShowToast(t('forum.toasts.postDeleted'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to delete post';
      onShowToast(msg, 'error');
    }
  };

  // Open Create Category Modal
  const handleOpenCreateCategory = () => {
    setCategoryForm({
      slug: '',
      name: '',
      description: '',
      position: categories.length + 1,
      is_private: false,
    });
    setCategoryModal({ mode: 'create' });
  };

  // Open Edit Category Modal
  const handleOpenEditCategory = (cat: ForumCategory) => {
    setCategoryForm({
      slug: cat.slug,
      name: cat.name,
      description: cat.description,
      position: cat.position,
      is_private: cat.is_private,
    });
    setCategoryModal({ mode: 'edit', category: cat });
  };

  // Save Category
  const handleSaveCategory = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!categoryModal) return;
    setIsSubmittingCategory(true);
    try {
      if (categoryModal.mode === 'create') {
        const created = await aegisApi.createForumCategory(categoryForm);
        setCategories((prev) => [...prev, created]);
        onShowToast(t('forum.toasts.categoryCreated'), 'success');
      } else if (categoryModal.category) {
        const updated = await aegisApi.updateForumCategory(
          categoryModal.category.slug,
          categoryForm
        );
        setCategories((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
        onShowToast(t('forum.toasts.categoryUpdated'), 'success');
      }
      setCategoryModal(null);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to save category';
      onShowToast(msg, 'error');
    } finally {
      setIsSubmittingCategory(false);
    }
  };

  // Delete Category
  const handleDeleteCategory = async (cat: ForumCategory) => {
    if (!window.confirm(t('forum.confirmDeleteCategory', { name: cat.name }))) {
      return;
    }
    try {
      await aegisApi.deleteForumCategory(cat.slug);
      setCategories((prev) => prev.filter((c) => c.id !== cat.id));
      onShowToast(t('forum.toasts.categoryDeleted'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to delete category';
      onShowToast(msg, 'error');
    }
  };

  const getCategoryName = (categoryId: string) => {
    return categories.find((c) => c.id === categoryId)?.name || categoryId;
  };

  const getCategorySlug = (categoryId: string) => {
    return categories.find((c) => c.id === categoryId)?.slug || 'general';
  };

  return (
    <main className={shop.main}>
      <div className={shop.column}>
        <div className={shop.heading}>
          <div className={shop.title}>
            <MessagesSquare size={22} />
            <h1>{t('forum.title')}</h1>
            <span className={shop.count}>
              {activeTab === 'threads'
                ? t('forum.threadCount', { count: threads.length })
                : t('forum.categoryCount', { count: categories.length })}
            </span>
          </div>

          <div className={shop.headingActions}>
            {activeTab === 'categories' ? (
              isAdmin ? (
                <Button variant="primary" onClick={handleOpenCreateCategory}>
                  <Plus size={14} />
                  <span>{t('forum.newCategory')}</span>
                </Button>
              ) : (
                <span className={shop.muted}>{t('common.requiresAdmin')}</span>
              )
            ) : (
              <Button
                variant="secondary"
                onClick={loadThreads}
                disabled={loading}
                title={t('forum.refresh')}
              >
                <RefreshCw size={14} className={loading ? 'animate-spin' : undefined} />
                <span>{t('forum.refresh')}</span>
              </Button>
            )}
          </div>
        </div>

        {error && <div className={cx(shop.notice, shop.error)}>{error}</div>}

        {/* Sub Tabs */}
        <div className={styles.subTabs}>
          <button
            className={cx(styles.subTab, activeTab === 'threads' && styles.active)}
            onClick={() => setActiveTab('threads')}
          >
            <MessageSquare size={16} />
            <span>{t('forum.tabs.threads')}</span>
          </button>
          <button
            className={cx(styles.subTab, activeTab === 'categories' && styles.active)}
            onClick={() => setActiveTab('categories')}
          >
            <FolderPlus size={16} />
            <span>{t('forum.tabs.categories')}</span>
          </button>
        </div>

        {/* TAB 1: Threads Moderation */}
        {activeTab === 'threads' && (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
              <label
                htmlFor="forum-cat-filter"
                style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}
              >
                {t('forum.filterCategory')}:
              </label>
              <select
                id="forum-cat-filter"
                className={styles.categorySelect}
                value={selectedCategorySlug}
                onChange={(e) => setSelectedCategorySlug(e.target.value)}
              >
                <option value="all">{t('forum.allCategories')}</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.slug}>
                    {c.name}
                  </option>
                ))}
              </select>
            </div>

            <div className={shop.tableWrap}>
              <table className={shop.table}>
                <thead>
                  <tr>
                    <th>{t('forum.table.thread')}</th>
                    <th>{t('forum.table.category')}</th>
                    <th>{t('forum.table.author')}</th>
                    <th>{t('forum.table.replies')}</th>
                    <th>{t('forum.table.lastActive')}</th>
                    <th style={{ textAlign: 'right' }}>{t('forum.table.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {threads.length === 0 ? (
                    <tr>
                      <td
                        colSpan={6}
                        style={{
                          textAlign: 'center',
                          padding: '32px 0',
                          color: 'var(--text-muted)',
                        }}
                      >
                        {loading ? t('forum.loading') : t('forum.noThreads')}
                      </td>
                    </tr>
                  ) : (
                    threads.map((thread) => (
                      <tr key={thread.id}>
                        <td className={styles.threadTitleCell}>
                          <div className={styles.threadTitle}>
                            {thread.is_pinned && (
                              <span className={styles.badgePinned}>
                                <Pin size={10} /> PIN
                              </span>
                            )}
                            {thread.is_locked && (
                              <span className={styles.badgeLocked}>
                                <Lock size={10} /> LOCKED
                              </span>
                            )}
                            <span>{thread.title}</span>
                          </div>
                          <div className={styles.threadMeta}>
                            <span>Created {formatDateTime(thread.created_at)}</span>
                          </div>
                        </td>
                        <td>{getCategoryName(thread.category_id)}</td>
                        <td>{thread.author_name}</td>
                        <td>{thread.post_count}</td>
                        <td>{formatDateTime(thread.last_post_at)}</td>
                        <td>
                          <div
                            className={styles.actionsCell}
                            style={{ justifyContent: 'flex-end' }}
                          >
                            {/* Pin / Unpin */}
                            <button
                              className={cx(styles.actionBtn, thread.is_pinned && styles.active)}
                              onClick={() => handleTogglePin(thread)}
                              title={thread.is_pinned ? t('forum.unpin') : t('forum.pin')}
                            >
                              <Pin size={13} />
                            </button>

                            {/* Lock / Unlock */}
                            <button
                              className={cx(styles.actionBtn, thread.is_locked && styles.active)}
                              onClick={() => handleToggleLock(thread)}
                              title={thread.is_locked ? t('forum.unlock') : t('forum.lock')}
                            >
                              {thread.is_locked ? <Unlock size={13} /> : <Lock size={13} />}
                            </button>

                            {/* View Posts Modal */}
                            <button
                              className={styles.actionBtn}
                              onClick={() => handleViewPosts(thread)}
                              title={t('forum.viewPosts')}
                            >
                              <Eye size={13} />
                            </button>

                            {/* Public Link */}
                            <a
                              href={`/forum/${getCategorySlug(thread.category_id)}/${thread.id}`}
                              target="_blank"
                              rel="noreferrer"
                              className={styles.actionBtn}
                              title={t('forum.openPublic')}
                            >
                              <ExternalLink size={13} />
                            </a>

                            {/* Delete Thread */}
                            <button
                              className={cx(styles.actionBtn, styles.danger)}
                              onClick={() => handleDeleteThread(thread)}
                              title={t('forum.deleteThread')}
                            >
                              <Trash2 size={13} />
                            </button>
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </>
        )}

        {/* TAB 2: Categories Management */}
        {activeTab === 'categories' && (
          <div className={shop.tableWrap}>
            <table className={shop.table}>
              <thead>
                <tr>
                  <th>{t('forum.table.categoryName')}</th>
                  <th>{t('forum.table.slug')}</th>
                  <th>{t('forum.table.description')}</th>
                  <th>{t('forum.table.position')}</th>
                  {isAdmin && <th style={{ textAlign: 'right' }}>{t('forum.table.actions')}</th>}
                </tr>
              </thead>
              <tbody>
                {categories.length === 0 ? (
                  <tr>
                    <td
                      colSpan={isAdmin ? 5 : 4}
                      style={{ textAlign: 'center', padding: '32px 0', color: 'var(--text-muted)' }}
                    >
                      {t('forum.noCategories')}
                    </td>
                  </tr>
                ) : (
                  categories.map((cat) => (
                    <tr key={cat.id}>
                      <td style={{ fontWeight: 600 }}>{cat.name}</td>
                      <td>
                        <code>{cat.slug}</code>
                      </td>
                      <td style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>
                        {cat.description || '-'}
                      </td>
                      <td>{cat.position}</td>
                      {isAdmin && (
                        <td>
                          <div
                            className={styles.actionsCell}
                            style={{ justifyContent: 'flex-end' }}
                          >
                            <button
                              className={styles.actionBtn}
                              onClick={() => handleOpenEditCategory(cat)}
                              title={t('forum.editCategory')}
                            >
                              <Edit3 size={13} />
                              <span>{t('forum.edit')}</span>
                            </button>
                            <button
                              className={cx(styles.actionBtn, styles.danger)}
                              onClick={() => handleDeleteCategory(cat)}
                              title={t('forum.deleteCategory')}
                            >
                              <Trash2 size={13} />
                              <span>{t('forum.delete')}</span>
                            </button>
                          </div>
                        </td>
                      )}
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Posts Viewer & Moderation Modal */}
      {postsModalThread && (
        <Modal
          title={t('forum.postsModalTitle', { title: postsModalThread.title })}
          icon={<MessageSquare size={18} />}
          onClose={() => setPostsModalThread(null)}
        >
          {loadingPosts ? (
            <p style={{ textAlign: 'center', padding: '24px 0', color: 'var(--text-muted)' }}>
              {t('forum.loading')}
            </p>
          ) : threadPosts.length === 0 ? (
            <p style={{ textAlign: 'center', padding: '24px 0', color: 'var(--text-muted)' }}>
              {t('forum.noPosts')}
            </p>
          ) : (
            <div className={styles.postsList}>
              {threadPosts.map((post) => (
                <div key={post.id} className={styles.postItem}>
                  <div className={styles.postHeader}>
                    <div>
                      <strong>#{post.post_number}</strong> by <strong>{post.author_name}</strong>
                      {post.ip && (
                        <span style={{ marginLeft: 8, opacity: 0.6 }}>(IP: {post.ip})</span>
                      )}
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <span>{formatDateTime(post.created_at)}</span>
                      {!post.is_deleted && post.post_number > 1 && (
                        <button
                          className={cx(styles.actionBtn, styles.danger)}
                          onClick={() => handleDeletePost(post)}
                          title={t('forum.deletePost')}
                        >
                          <Trash2 size={12} />
                        </button>
                      )}
                    </div>
                  </div>
                  <div className={cx(styles.postBody, post.is_deleted && styles.postDeleted)}>
                    {post.body}
                  </div>
                </div>
              ))}
            </div>
          )}
        </Modal>
      )}

      {/* Create / Edit Category Modal */}
      {categoryModal && (
        <Modal
          title={
            categoryModal.mode === 'create'
              ? t('forum.modalCreateCategory')
              : t('forum.modalEditCategory')
          }
          icon={<FolderPlus size={18} />}
          onClose={() => setCategoryModal(null)}
        >
          <form onSubmit={handleSaveCategory} className={styles.modalForm}>
            <div className={styles.formGroup}>
              <label htmlFor="cat-name">{t('forum.categoryName')}</label>
              <input
                id="cat-name"
                type="text"
                required
                className={styles.formInput}
                value={categoryForm.name}
                onChange={(e) => setCategoryForm({ ...categoryForm, name: e.target.value })}
                placeholder="General Discussion"
              />
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="cat-slug">{t('forum.categorySlug')}</label>
              <input
                id="cat-slug"
                type="text"
                required
                className={styles.formInput}
                value={categoryForm.slug}
                onChange={(e) =>
                  setCategoryForm({
                    ...categoryForm,
                    slug: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, '-'),
                  })
                }
                placeholder="general"
              />
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="cat-desc">{t('forum.categoryDescription')}</label>
              <textarea
                id="cat-desc"
                rows={3}
                className={styles.formInput}
                value={categoryForm.description}
                onChange={(e) => setCategoryForm({ ...categoryForm, description: e.target.value })}
                placeholder="Describe what this category is for..."
              />
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="cat-pos">{t('forum.categoryPosition')}</label>
              <input
                id="cat-pos"
                type="number"
                className={styles.formInput}
                value={categoryForm.position}
                onChange={(e) =>
                  setCategoryForm({ ...categoryForm, position: Number(e.target.value) })
                }
              />
            </div>

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 12 }}>
              <Button variant="secondary" type="button" onClick={() => setCategoryModal(null)}>
                {t('common.cancel')}
              </Button>
              <Button variant="primary" type="submit" disabled={isSubmittingCategory}>
                {isSubmittingCategory ? t('common.saving') : t('common.save')}
              </Button>
            </div>
          </form>
        </Modal>
      )}
    </main>
  );
};
