import React, { useEffect, useState } from 'react';
import {
  CheckCircle2,
  Edit3,
  ExternalLink,
  Eye,
  Flag,
  FolderPlus,
  Lock,
  MessagesSquare,
  MessageSquare,
  Pin,
  Plus,
  RefreshCw,
  Settings,
  ShieldAlert,
  Trash2,
  Unlock,
  XCircle,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi, ApiError } from '../api/client';
import type {
  CommentStatus,
  CreateForumCategoryPayload,
  ForumCategory,
  ForumModerationSettings,
  ForumPost,
  ForumReport,
  ForumThread,
} from '../types';
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

type TabMode = 'threads' | 'reports' | 'categories' | 'settings';

export const ForumManager: React.FC<ForumManagerProps> = ({ isAdmin, onShowToast }) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<TabMode>('threads');
  const [categories, setCategories] = useState<ForumCategory[]>([]);
  const [selectedCategorySlug, setSelectedCategorySlug] = useState<string>('all');
  const [selectedStatus, setSelectedStatus] = useState<string>('all');
  const [threads, setThreads] = useState<ForumThread[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // Reports state
  const [reports, setReports] = useState<ForumReport[]>([]);
  const [reportFilter, setReportFilter] = useState<string>('open');
  const [loadingReports, setLoadingReports] = useState<boolean>(false);

  // Moderation settings state
  const [modSettings, setModSettings] = useState<ForumModerationSettings | null>(null);
  const [savingSettings, setSavingSettings] = useState<boolean>(false);

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
        status: selectedStatus,
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

  // Load reports
  const loadReports = async () => {
    setLoadingReports(true);
    setError(null);
    try {
      const loaded = await aegisApi.listForumReports({
        status: reportFilter,
        limit: 100,
      });
      setReports(loaded);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load reports';
      setError(msg);
    } finally {
      setLoadingReports(false);
    }
  };

  // Load moderation settings
  const loadModSettings = async () => {
    setError(null);
    try {
      const loaded = await aegisApi.getForumSettings();
      setModSettings(loaded);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load settings';
      setError(msg);
    }
  };

  useEffect(() => {
    loadCategories();
  }, []);

  useEffect(() => {
    if (activeTab === 'threads') {
      loadThreads();
    } else if (activeTab === 'reports') {
      loadReports();
    } else if (activeTab === 'settings') {
      loadModSettings();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, selectedCategorySlug, selectedStatus, reportFilter]);

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

  // Handle Thread Moderation Status
  const handleModerateThread = async (thread: ForumThread, status: CommentStatus) => {
    try {
      const updated = await aegisApi.moderateForumThread(thread.id, status);
      setThreads((prev) => prev.map((t) => (t.id === thread.id ? updated : t)));
      onShowToast(t('forum.toasts.threadModerated'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to moderate thread';
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
      const posts = await aegisApi.listForumPosts(thread.id, { limit: 100, status: 'all' });
      setThreadPosts(posts);
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to load posts';
      onShowToast(msg, 'error');
    } finally {
      setLoadingPosts(false);
    }
  };

  // Handle Post Moderation Status
  const handleModeratePost = async (post: ForumPost, status: CommentStatus) => {
    try {
      const updated = await aegisApi.moderateForumPost(post.id, status);
      setThreadPosts((prev) => prev.map((p) => (p.id === post.id ? updated : p)));
      onShowToast(t('forum.toasts.postModerated'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to moderate post';
      onShowToast(msg, 'error');
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

  // Handle Report Resolution
  const handleResolveReport = async (
    report: ForumReport,
    nextStatus: 'open' | 'resolved' | 'dismissed'
  ) => {
    try {
      const updated = await aegisApi.resolveForumReport(report.id, nextStatus);
      setReports((prev) =>
        reportFilter !== 'all' && updated.status !== reportFilter
          ? prev.filter((r) => r.id !== report.id)
          : prev.map((r) => (r.id === report.id ? updated : r))
      );
      onShowToast(t('forum.toasts.reportUpdated'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to update report';
      onShowToast(msg, 'error');
    }
  };

  // Handle Moderation Settings Update
  const handleUpdatePolicy = async (patch: {
    require_approval?: boolean;
    auto_approve_known?: boolean;
  }) => {
    if (!modSettings || !isAdmin) return;
    setSavingSettings(true);
    try {
      const updated = await aegisApi.updateForumSettings({
        require_approval:
          patch.require_approval !== undefined
            ? patch.require_approval
            : modSettings.policy.require_approval,
        auto_approve_known:
          patch.auto_approve_known !== undefined
            ? patch.auto_approve_known
            : modSettings.policy.auto_approve_known,
      });
      setModSettings(updated);
      onShowToast(t('forum.toasts.settingsSaved'), 'success');
    } catch (err: unknown) {
      const msg = err instanceof ApiError ? err.message : 'Failed to save settings';
      onShowToast(msg, 'error');
    } finally {
      setSavingSettings(false);
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

  const renderStatusBadge = (status?: CommentStatus) => {
    const st = status || 'approved';
    if (st === 'pending') {
      return <span className={cx(shop.badge, shop.warn)}>{t('forum.status.pending')}</span>;
    }
    if (st === 'spam') {
      return <span className={cx(shop.badge, shop.bad)}>{t('forum.status.spam')}</span>;
    }
    return <span className={cx(shop.badge, shop.good)}>{t('forum.status.approved')}</span>;
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
                : activeTab === 'categories'
                  ? t('forum.categoryCount', { count: categories.length })
                  : ''}
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
                onClick={
                  activeTab === 'reports'
                    ? loadReports
                    : activeTab === 'settings'
                      ? loadModSettings
                      : loadThreads
                }
                disabled={loading || loadingReports}
                title={t('forum.refresh')}
              >
                <RefreshCw
                  size={14}
                  className={loading || loadingReports ? 'animate-spin' : undefined}
                />
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
            className={cx(styles.subTab, activeTab === 'reports' && styles.active)}
            onClick={() => setActiveTab('reports')}
          >
            <Flag size={16} />
            <span>{t('forum.tabs.reports')}</span>
          </button>
          <button
            className={cx(styles.subTab, activeTab === 'categories' && styles.active)}
            onClick={() => setActiveTab('categories')}
          >
            <FolderPlus size={16} />
            <span>{t('forum.tabs.categories')}</span>
          </button>
          <button
            className={cx(styles.subTab, activeTab === 'settings' && styles.active)}
            onClick={() => setActiveTab('settings')}
          >
            <Settings size={16} />
            <span>{t('forum.tabs.settings')}</span>
          </button>
        </div>

        {/* TAB 1: Threads Moderation */}
        {activeTab === 'threads' && (
          <>
            <div
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                alignItems: 'center',
                gap: 14,
                marginBottom: 10,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
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

              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <label
                  htmlFor="forum-status-filter"
                  style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}
                >
                  {t('forum.filterStatus')}:
                </label>
                <select
                  id="forum-status-filter"
                  className={styles.categorySelect}
                  value={selectedStatus}
                  onChange={(e) => setSelectedStatus(e.target.value)}
                >
                  <option value="all">{t('forum.status.all')}</option>
                  <option value="approved">{t('forum.status.approved')}</option>
                  <option value="pending">{t('forum.status.pending')}</option>
                  <option value="spam">{t('forum.status.spam')}</option>
                </select>
              </div>
            </div>

            <div className={shop.tableWrap}>
              <table className={shop.table}>
                <thead>
                  <tr>
                    <th>{t('forum.table.thread')}</th>
                    <th>{t('forum.table.category')}</th>
                    <th>{t('forum.table.author')}</th>
                    <th>{t('forum.table.status')}</th>
                    <th>{t('forum.table.replies')}</th>
                    <th>{t('forum.table.lastActive')}</th>
                    <th style={{ textAlign: 'right' }}>{t('forum.table.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {threads.length === 0 ? (
                    <tr>
                      <td
                        colSpan={7}
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
                            <span className={styles.threadTitleText}>{thread.title}</span>
                          </div>
                          <div className={styles.threadMeta}>
                            <span>Created {formatDateTime(thread.created_at)}</span>
                          </div>
                        </td>
                        <td className={styles.nowrapCell}>{getCategoryName(thread.category_id)}</td>
                        <td className={styles.nowrapCell}>{thread.author_name}</td>
                        <td className={styles.nowrapCell}>{renderStatusBadge(thread.status)}</td>
                        <td className={styles.nowrapCell}>{thread.post_count}</td>
                        <td className={styles.nowrapCell}>{formatDateTime(thread.last_post_at)}</td>
                        <td>
                          <div
                            className={styles.actionsCell}
                            style={{ justifyContent: 'flex-end' }}
                          >
                            {/* Approve / Hold / Spam */}
                            {thread.status !== 'approved' && (
                              <button
                                className={styles.actionBtn}
                                onClick={() => handleModerateThread(thread, 'approved')}
                                title={t('forum.approve')}
                              >
                                <CheckCircle2 size={13} />
                              </button>
                            )}
                            {thread.status !== 'spam' && (
                              <button
                                className={styles.actionBtn}
                                onClick={() => handleModerateThread(thread, 'spam')}
                                title={t('forum.markSpam')}
                              >
                                <ShieldAlert size={13} />
                              </button>
                            )}

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

        {/* TAB 2: Reports */}
        {activeTab === 'reports' && (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
              <label
                htmlFor="forum-report-filter"
                style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}
              >
                {t('forum.reports.filter')}:
              </label>
              <select
                id="forum-report-filter"
                className={styles.categorySelect}
                value={reportFilter}
                onChange={(e) => setReportFilter(e.target.value)}
              >
                <option value="open">{t('forum.reports.open')}</option>
                <option value="resolved">{t('forum.reports.resolved')}</option>
                <option value="dismissed">{t('forum.reports.dismissed')}</option>
                <option value="all">{t('forum.reports.all')}</option>
              </select>
            </div>

            <div className={shop.tableWrap}>
              <table className={shop.table}>
                <thead>
                  <tr>
                    <th>{t('forum.reports.target')}</th>
                    <th>{t('forum.reports.reason')}</th>
                    <th>{t('forum.reports.reporter')}</th>
                    <th>{t('forum.table.status')}</th>
                    <th>{t('forum.reports.date')}</th>
                    <th style={{ textAlign: 'right' }}>{t('forum.table.actions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {reports.length === 0 ? (
                    <tr>
                      <td
                        colSpan={6}
                        style={{
                          textAlign: 'center',
                          padding: '32px 0',
                          color: 'var(--text-muted)',
                        }}
                      >
                        {loadingReports ? t('forum.loading') : t('forum.reports.empty')}
                      </td>
                    </tr>
                  ) : (
                    reports.map((report) => (
                      <tr key={report.id}>
                        <td>
                          <code>
                            {report.target_type}:{report.target_id.slice(0, 8)}
                          </code>
                        </td>
                        <td>{report.reason}</td>
                        <td>{report.reporter_id || report.reporter_ip || '-'}</td>
                        <td>{report.status}</td>
                        <td>{formatDateTime(report.created_at)}</td>
                        <td>
                          <div
                            className={styles.actionsCell}
                            style={{ justifyContent: 'flex-end' }}
                          >
                            {report.status !== 'resolved' && (
                              <button
                                className={styles.actionBtn}
                                onClick={() => handleResolveReport(report, 'resolved')}
                                title={t('forum.reports.resolve')}
                              >
                                <CheckCircle2 size={13} />
                                <span>{t('forum.reports.resolve')}</span>
                              </button>
                            )}
                            {report.status !== 'dismissed' && (
                              <button
                                className={styles.actionBtn}
                                onClick={() => handleResolveReport(report, 'dismissed')}
                                title={t('forum.reports.dismiss')}
                              >
                                <XCircle size={13} />
                                <span>{t('forum.reports.dismiss')}</span>
                              </button>
                            )}
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

        {/* TAB 3: Categories Management */}
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

        {/* TAB 4: Moderation Settings */}
        {activeTab === 'settings' && modSettings && (
          <div className={shop.tableWrap} style={{ padding: 20 }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: 10, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={modSettings.policy.require_approval}
                  disabled={!isAdmin || savingSettings}
                  onChange={(e) => handleUpdatePolicy({ require_approval: e.target.checked })}
                />
                <span>{t('forum.moderation.requireApproval')}</span>
              </label>

              <label style={{ display: 'flex', alignItems: 'center', gap: 10, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={modSettings.policy.auto_approve_known}
                  disabled={!isAdmin || savingSettings || !modSettings.policy.require_approval}
                  onChange={(e) => handleUpdatePolicy({ auto_approve_known: e.target.checked })}
                />
                <span>{t('forum.moderation.autoApproveKnown')}</span>
              </label>

              <hr style={{ border: 'none', borderTop: '1px solid var(--border-subtle)' }} />

              <div>
                <h3 style={{ fontSize: '0.95rem', margin: '0 0 8px' }}>
                  {t('forum.moderation.ngWordsTitle')}
                </h3>
                <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', margin: '4px 0' }}>
                  {t('forum.moderation.maxLinks', { count: modSettings.config.max_links })} /{' '}
                  {t('forum.moderation.maxPerIp', {
                    count: modSettings.config.max_per_ip_per_hour,
                  })}
                </p>
                <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', margin: '4px 0' }}>
                  <strong>{t('forum.moderation.holdWords')}:</strong>{' '}
                  {modSettings.config.hold_words.length > 0
                    ? modSettings.config.hold_words.join(', ')
                    : t('forum.moderation.none')}
                </p>
                <p style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', margin: '4px 0' }}>
                  <strong>{t('forum.moderation.spamWords')}:</strong>{' '}
                  {modSettings.config.spam_words.length > 0
                    ? modSettings.config.spam_words.join(', ')
                    : t('forum.moderation.none')}
                </p>
              </div>
            </div>
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
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <strong>#{post.post_number}</strong> by <strong>{post.author_name}</strong>
                      {renderStatusBadge(post.status)}
                      {post.ip && <span style={{ opacity: 0.6 }}>(IP: {post.ip})</span>}
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <span>{formatDateTime(post.created_at)}</span>
                      {post.status !== 'approved' && (
                        <button
                          className={styles.actionBtn}
                          onClick={() => handleModeratePost(post, 'approved')}
                          title={t('forum.approve')}
                        >
                          <CheckCircle2 size={12} />
                        </button>
                      )}
                      {post.status !== 'spam' && (
                        <button
                          className={styles.actionBtn}
                          onClick={() => handleModeratePost(post, 'spam')}
                          title={t('forum.markSpam')}
                        >
                          <ShieldAlert size={12} />
                        </button>
                      )}
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
