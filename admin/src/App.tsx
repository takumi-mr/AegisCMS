import React, { useState, useEffect, useRef } from 'react';
import {
  Plus,
  Hash,
  AlignLeft,
  Code2,
  Image as ImageIcon,
  CheckCircle2,
  AlertCircle,
  Tags,
  LayoutGrid,
  FileText,
  Clipboard,
  List,
  PenSquare,
  Eye,
  FileCode,
  Table,
  ListTree,
  Quote,
  Minus,
  Info,
  Puzzle,
  ListCollapse,
  MessageSquare,
  MessageSquareOff,
  Mail,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { AnalyticsView } from './components/AnalyticsView';
import { Header, type AppView } from './components/Header';
import { ProductManager } from './components/ProductManager';
import { CommentManager } from './components/CommentManager';
import { ContactInbox } from './components/ContactInbox';
import { ForumManager } from './components/ForumManager';
import { OrderManager } from './components/OrderManager';
import { ScheduleCalendar } from './components/ScheduleCalendar';
import { ProfileModal } from './components/ProfileModal';
import { LoginView, SessionCheckingScreen } from './components/LoginView';
import { SettingsView, type SettingsSection } from './components/SettingsView';
import { TagPicker } from './components/TagPicker';
import { ContentList } from './components/ContentList';
import { NO_FILTERS, type ContentListFilters } from './utils/contentFilters';
import {
  ContentVersionHistory,
  type ContentVersionSummary,
} from './components/ContentVersionHistory';
import { BlockItem } from './components/BlockItem';
import { Preview } from './components/Preview';
import { MarkdownEditor } from './components/MarkdownEditor';
import { PasteMarkdownModal } from './components/PasteMarkdownModal';
import { aegisApi, setUnauthorizedHandler } from './api/client';
import { setSessionToken } from './api/session';
import { isSsoSession, takeLoginRedirect } from './utils/loginRedirect';
import { takeInviteToken, takeResetToken, takeVerifyToken } from './utils/verifyEmail';
import type {
  BlockNode,
  ContactForm,
  ContentItem,
  ContentSummary,
  PluginBlockDefinition,
  SessionInfo,
  Tag,
  UserAccount,
} from './types';
import { defaultAttributes, localize } from './utils/pluginBlocks';
import { categoryIdOf } from './utils/category';
import { canEdit, isAdmin } from './utils/roles';
import { cx } from './utils/cx';
import { toDateTimeLocal } from './utils/format';
import {
  blocksToMarkdown,
  markdownToBlocks,
  isMarkdownLike,
  TOC_DEFAULT_MAX_LEVEL,
} from './utils/markdown';
import { Button } from './components/ui/Button';
import { Field, FieldRow } from './components/ui/Field';
import styles from './App.module.css';

/** Content states the editor offers (`archived` and others open as a draft) */
type EditorStatus = 'draft' | 'scheduled' | 'published';

/** Messages shown above the login form */
type LoginNotice = 'unreachable' | 'expired' | 'signedOut';

/** `#comments`, `#forum` or `#contact` (links in notifications) open the matching view */
/** Items per page of the content list */
const LIST_PAGE_SIZE = 20;

const takeInitialView = (): AppView => {
  if (window.location.hash === '#comments') {
    window.history.replaceState(null, '', window.location.pathname + window.location.search);
    return 'comments';
  }
  if (window.location.hash === '#forum') {
    window.history.replaceState(null, '', window.location.pathname + window.location.search);
    return 'forum';
  }
  if (window.location.hash === '#contact') {
    window.history.replaceState(null, '', window.location.pathname + window.location.search);
    return 'contact';
  }
  return 'editor';
};

export const App: React.FC = () => {
  const { t, i18n } = useTranslation();
  // undefined = still checking the stored token, null = logged out
  const [session, setSession] = useState<SessionInfo | null | undefined>(undefined);
  const [loginNotice, setLoginNotice] = useState<LoginNotice | null>(null);
  // The signed-in user's account (avatar, profile); null until loaded or for unknown accounts
  const [profile, setProfile] = useState<UserAccount | null>(null);
  const [isProfileOpen, setIsProfileOpen] = useState<boolean>(false);
  // Single sign-on returns here with the session token (stored before the session check) or an error
  const [ssoError] = useState<string | null>(takeLoginRedirect);
  const [verifyToken] = useState<string | null>(takeVerifyToken);
  const [resetToken] = useState<string | null>(takeResetToken);
  const [inviteToken] = useState<string | null>(takeInviteToken);
  const [view, setView] = useState<AppView>(takeInitialView);
  // Hidden until the server says the shop is on ([commerce].enabled)
  const [shopEnabled, setShopEnabled] = useState<boolean>(false);
  /** Custom blocks of the enabled plugins, offered in the block toolbar */
  const [blockDefinitions, setBlockDefinitions] = useState<PluginBlockDefinition[]>([]);
  // Hidden until the server says comments are on ([comments].enabled)
  const [commentsEnabled, setCommentsEnabled] = useState<boolean>(false);
  // `[tags]` on the server: the tags of the opened content are picked in the editor
  const [tagsEnabled, setTagsEnabled] = useState<boolean>(false);
  const [tags, setTags] = useState<Tag[]>([]);
  const [contentTags, setContentTags] = useState<Tag[]>([]);
  const tagsRequest = useRef(0);
  const [commentContentTypes, setCommentContentTypes] = useState<string[]>([]);
  const [commentsOpen, setCommentsOpen] = useState<boolean>(true);
  const [togglingComments, setTogglingComments] = useState<boolean>(false);
  // Hidden until the server says forum is on ([forum].enabled)
  const [forumEnabled, setForumEnabled] = useState<boolean>(false);
  // Contact forms ([contact].enabled); empty until the server lists them
  const [contactForms, setContactForms] = useState<ContactForm[]>([]);
  // Hidden until the server says analytics are on ([analytics].enabled)
  const [analyticsEnabled, setAnalyticsEnabled] = useState<boolean>(false);
  const [isOnline, setIsOnline] = useState<boolean>(false);
  const [contentType, setContentType] = useState<'article' | 'page'>('article');
  const [layout, setLayout] = useState<'default' | 'blank'>('default');
  // The list shows one page of summaries; the server searches, filters and pages it
  const [contents, setContents] = useState<ContentSummary[]>([]);
  const [contentsTotal, setContentsTotal] = useState(0);
  const [contentsLoading, setContentsLoading] = useState(false);
  const [listFilters, setListFilters] = useState<ContentListFilters>(NO_FILTERS);
  const [listPage, setListPage] = useState(1);
  const listRequest = useRef(0);
  const selectRequest = useRef(0);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [contentVersions, setContentVersions] = useState<ContentVersionSummary[]>([]);
  const [loadingContentVersions, setLoadingContentVersions] = useState(false);
  const contentVersionRequest = useRef(0);
  const [categories, setCategories] = useState<ContentSummary[]>([]);
  const [categoryId, setCategoryId] = useState<string>('');
  const [settingsSection, setSettingsSection] = useState<SettingsSection>('themes');
  const [mobilePane, setMobilePane] = useState<'list' | 'editor' | 'preview'>('editor');

  // Editor Form State
  const [title, setTitle] = useState<string>('Next-Gen WebAssembly CMS with Aegis');
  const [slug, setSlug] = useState<string>('next-gen-wasm-cms');
  const [status, setStatus] = useState<EditorStatus>('published');
  // Scheduled publishing time (`datetime-local` value, local time)
  const [publishAt, setPublishAt] = useState<string>('');
  // Scheduled unpublishing time (`datetime-local` value, local time)
  const [unpublishAt, setUnpublishAt] = useState<string>('');
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

  // Markdown & Paste Support State
  const [editorMode, setEditorMode] = useState<'blocks' | 'markdown'>('blocks');
  const [markdownText, setMarkdownText] = useState<string>(() =>
    blocksToMarkdown([
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
    ])
  );
  const [isPasteModalOpen, setIsPasteModalOpen] = useState<boolean>(false);

  const [isSaving, setIsSaving] = useState<boolean>(false);
  const [toast, setToast] = useState<{ message: string; type: 'success' | 'error' } | null>(null);

  const showToast = (message: string, type: 'success' | 'error' = 'success') => {
    setToast({ message, type });
    setTimeout(() => setToast(null), 4000);
  };

  const loadCategories = async () => {
    try {
      setCategories(await aegisApi.getAllContentSummaries('category'));
    } catch (err) {
      console.warn('Could not load categories:', err);
    }
  };

  /** Loads one page of the list (the current type, filters and page unless given) */
  const loadContents = async (
    options: { type?: 'article' | 'page'; filters?: ContentListFilters; page?: number } = {}
  ): Promise<void> => {
    const type = options.type ?? contentType;
    const filters = options.filters ?? listFilters;
    const page = options.page ?? listPage;
    // Only the newest request may show its answer
    const requestId = ++listRequest.current;
    setContentsLoading(true);
    try {
      const result = await aegisApi.listContentSummaries(type, {
        q: filters.q.trim(),
        category: type === 'article' ? filters.category : '',
        tag: filters.tag,
        status: filters.status,
        limit: LIST_PAGE_SIZE,
        offset: (page - 1) * LIST_PAGE_SIZE,
      });
      if (requestId !== listRequest.current) return;
      const lastPage = Math.max(1, Math.ceil(result.total / LIST_PAGE_SIZE));
      if (page > lastPage) {
        // The page is gone (its last item was deleted or moved): show the last page instead
        await loadContents({ type, filters, page: lastPage });
        return;
      }
      setContents(result.items);
      setContentsTotal(result.total);
      setListPage(page);
    } catch (err) {
      if (requestId === listRequest.current) console.warn('Could not load contents:', err);
    } finally {
      if (requestId === listRequest.current) setContentsLoading(false);
    }
  };

  const handleFiltersChange = (filters: ContentListFilters) => {
    setListFilters(filters);
    setListPage(1);
    void loadContents({ filters, page: 1 });
  };

  const handlePageChange = (page: number) => {
    setListPage(page);
    void loadContents({ page });
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
        setLoginNotice('unreachable');
        setSession(null);
      });

    setUnauthorizedHandler(() => {
      setSessionToken('');
      setSession(null);
      setProfile(null);
      setIsProfileOpen(false);
      setLoginNotice('expired');
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
  const sessionUser = session?.user_id;
  useEffect(() => {
    if (!sessionId) return;
    loadContents();
    loadCategories();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    aegisApi
      .listBlocks()
      .then((blocks) => {
        if (!cancelled) setBlockDefinitions(blocks);
      })
      .catch((err) => console.warn('Could not load the plugin blocks:', err));
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    aegisApi
      .getShopConfig()
      .then((config) => {
        if (!cancelled) setShopEnabled(config.enabled);
      })
      .catch((err) => console.warn('Could not load the shop settings:', err));
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    // The tag list answers 404 when tags are disabled
    aegisApi
      .listTags({ limit: 200 })
      .then((res) => {
        if (!cancelled) {
          setTagsEnabled(true);
          setTags(res.tags);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setTagsEnabled(false);
          setTags([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId || !canEdit(session ?? null)) return;
    let cancelled = false;
    // The settings answer 404 when comments are disabled
    aegisApi
      .getCommentSettings()
      .then((settings) => {
        if (!cancelled) {
          setCommentsEnabled(true);
          setCommentContentTypes(settings.content_types ?? ['article']);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setCommentsEnabled(false);
          setCommentContentTypes([]);
        }
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId || !canEdit(session ?? null)) return;
    let cancelled = false;
    aegisApi
      .listForumCategories()
      .then(() => {
        if (!cancelled) setForumEnabled(true);
      })
      .catch(() => {
        if (!cancelled) setForumEnabled(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    // Answers 404 when contact forms are disabled
    aegisApi
      .listContactForms()
      .then((forms) => {
        if (!cancelled) setContactForms(forms);
      })
      .catch(() => {
        if (!cancelled) setContactForms([]);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId) return;
    let cancelled = false;
    // Every signed-in role may read the summary; it answers 404 when analytics are disabled
    const today = new Date().toISOString().slice(0, 10);
    aegisApi
      .getAnalyticsSummary({ from: today, to: today, limit: 1 })
      .then(() => {
        if (!cancelled) setAnalyticsEnabled(true);
      })
      .catch(() => {
        if (!cancelled) setAnalyticsEnabled(false);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!sessionId || !sessionUser) return;
    let cancelled = false;
    aegisApi
      .getUser(sessionUser)
      .then((account) => {
        if (!cancelled) setProfile(account);
      })
      .catch((err) => console.warn('Could not load the profile:', err));
    return () => {
      cancelled = true;
    };
  }, [sessionId, sessionUser]);

  const handleLoggedIn = (loggedIn: SessionInfo) => {
    setSessionToken(loggedIn.id);
    setProfile(null);
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
    setProfile(null);
    setIsProfileOpen(false);
    setLoginNotice('signedOut');
    setView('editor');
    setContents([]);
    setContentsTotal(0);
    setListFilters(NO_FILTERS);
    setListPage(1);
    setCategories([]);
  };

  const handleCategoryCreated = (category: ContentItem) => {
    setCategories((prev) => [category, ...prev.filter((c) => c.id !== category.id)]);
    showToast(t('editor.toasts.categoryCreated', { title: category.title }));
  };

  /** Loads the tags of the opened content (nothing when tags are disabled) */
  const loadContentTags = (type: string, id: string) => {
    const requestId = ++tagsRequest.current;
    setContentTags([]);
    if (!tagsEnabled) return;
    aegisApi
      .getContentTags(type, id)
      .then((loaded) => {
        if (requestId === tagsRequest.current) setContentTags(loaded);
      })
      .catch((err: unknown) => {
        if (requestId === tagsRequest.current) {
          console.warn('Could not load the tags of the content:', err);
          showToast(t('tags.loadFailedForContent'), 'error');
        }
      });
  };

  const handleSelectContent = (item: ContentItem) => {
    setSelectedId(item.id);
    setCommentsOpen(item.comments_open !== false);
    setContentVersions([]);
    const requestId = ++contentVersionRequest.current;
    setLoadingContentVersions(true);
    void aegisApi
      .listContentVersions(item.content_type, item.id)
      .then((versions) => {
        if (requestId === contentVersionRequest.current) setContentVersions(versions);
      })
      .catch((error) => console.warn('Could not load content versions:', error))
      .finally(() => {
        if (requestId === contentVersionRequest.current) setLoadingContentVersions(false);
      });
    setTitle(item.title);
    setSlug(item.slug);
    setStatus(item.status === 'published' || item.status === 'scheduled' ? item.status : 'draft');
    setPublishAt(
      item.status === 'scheduled' && item.published_at ? toDateTimeLocal(item.published_at) : ''
    );
    setUnpublishAt(item.unpublish_at ? toDateTimeLocal(item.unpublish_at) : '');
    const itemBlocks = item.blocks || [];
    setBlocks(itemBlocks);
    setMarkdownText(blocksToMarkdown(itemBlocks));
    setCategoryId(categoryIdOf(item) ?? '');
    loadContentTags(item.content_type, item.id);
    const rawLayout = item.fields?.layout ?? (item as unknown as { layout?: string }).layout;
    setLayout(rawLayout === 'blank' ? 'blank' : 'default');
    setMobilePane('editor');
  };

  /** Opens an item of the list: the list has summaries only, so the whole item is fetched */
  const handleSelectSummary = async (summary: ContentSummary) => {
    const requestId = ++selectRequest.current;
    try {
      const item = await aegisApi.getContent(summary.content_type, summary.id);
      if (requestId === selectRequest.current) handleSelectContent(item);
    } catch (err: any) {
      if (requestId === selectRequest.current) {
        showToast(err.message || t('api.contentNotFound'), 'error');
      }
    }
  };

  const handleNew = () => {
    tagsRequest.current += 1;
    setContentTags([]);
    selectRequest.current += 1;
    contentVersionRequest.current += 1;
    setSelectedId(null);
    setCommentsOpen(true);
    setContentVersions([]);
    setLoadingContentVersions(false);
    setTitle(
      contentType === 'article' ? t('editor.defaults.untitled') : t('editor.defaults.untitledPage')
    );
    setSlug(`untitled-${Date.now().toString(36)}`);
    setStatus('draft');
    setPublishAt('');
    setUnpublishAt('');
    setCategoryId('');
    setLayout('default');
    const newBlocks: BlockNode[] = [
      { name: 'core/heading', attributes: { level: 2 }, text: t('editor.defaults.newHeading') },
      { name: 'core/paragraph', attributes: {}, text: '' },
    ];
    setBlocks(newBlocks);
    setMarkdownText(blocksToMarkdown(newBlocks));
    setMobilePane('editor');
  };

  const handleContentTypeChange = async (type: 'article' | 'page') => {
    if (type === contentType) return;
    tagsRequest.current += 1;
    setContentTags([]);
    setContentType(type);
    setSelectedId(null);
    setCommentsOpen(true);
    setTitle(
      type === 'article' ? t('editor.defaults.untitled') : t('editor.defaults.untitledPage')
    );
    setSlug(`untitled-${Date.now().toString(36)}`);
    setStatus('draft');
    setPublishAt('');
    setUnpublishAt('');
    setCategoryId('');
    setLayout('default');
    const newBlocks: BlockNode[] = [
      { name: 'core/heading', attributes: { level: 2 }, text: t('editor.defaults.newHeading') },
      { name: 'core/paragraph', attributes: {}, text: '' },
    ];
    setBlocks(newBlocks);
    setMarkdownText(blocksToMarkdown(newBlocks));
    setMobilePane('editor');
    // Another type is another list: start from its first page without the search and filters
    selectRequest.current += 1;
    setListFilters(NO_FILTERS);
    setListPage(1);
    void loadContents({ type, filters: NO_FILTERS, page: 1 });
  };

  // Block Manipulation
  const addBlock = (name: string, defaultText = '', defaultAttrs: Record<string, any> = {}) => {
    const next = [...blocks, { name, text: defaultText, attributes: defaultAttrs }];
    setBlocks(next);
    setMarkdownText(blocksToMarkdown(next));
  };

  const updateBlock = (index: number, updated: BlockNode) => {
    const next = [...blocks];
    next[index] = updated;
    setBlocks(next);
    setMarkdownText(blocksToMarkdown(next));
  };

  const moveBlock = (index: number, direction: 'up' | 'down') => {
    const next = [...blocks];
    const target = direction === 'up' ? index - 1 : index + 1;
    if (target < 0 || target >= next.length) return;
    const [removed] = next.splice(index, 1);
    next.splice(target, 0, removed);
    setBlocks(next);
    setMarkdownText(blocksToMarkdown(next));
  };

  const deleteBlock = (index: number) => {
    const next = blocks.filter((_, i) => i !== index);
    setBlocks(next);
    setMarkdownText(blocksToMarkdown(next));
  };

  // Markdown Mode Switching & Editing
  const handleSwitchMode = (newMode: 'blocks' | 'markdown') => {
    if (newMode === editorMode) return;
    if (newMode === 'markdown') {
      setMarkdownText(blocksToMarkdown(blocks));
      setEditorMode('markdown');
    } else {
      const parsed = markdownToBlocks(markdownText);
      setBlocks(parsed);
      setEditorMode('blocks');
    }
  };

  const handleMarkdownChange = (newText: string) => {
    setMarkdownText(newText);
    const parsed = markdownToBlocks(newText);
    setBlocks(parsed);
  };

  // Paste Handling for Block Canvas
  const handleCanvasPaste = (e: React.ClipboardEvent) => {
    const target = e.target as HTMLElement;
    const isInsideInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA';

    const text = e.clipboardData.getData('text');
    if (!text || !text.trim()) return;

    if (isInsideInput && !isMarkdownLike(text)) {
      return;
    }

    if (isMarkdownLike(text) || !isInsideInput) {
      e.preventDefault();
      const parsed = markdownToBlocks(text);
      if (parsed.length > 0) {
        const nextBlocks = [...blocks, ...parsed];
        setBlocks(nextBlocks);
        setMarkdownText(blocksToMarkdown(nextBlocks));
        showToast(t('editor.toasts.pasted', { count: parsed.length }));
      }
    }
  };

  const handlePasteInsert = (insertedBlocks: BlockNode[], mode: 'append' | 'replace') => {
    const nextBlocks = mode === 'replace' ? insertedBlocks : [...blocks, ...insertedBlocks];
    setBlocks(nextBlocks);
    setMarkdownText(blocksToMarkdown(nextBlocks));
    showToast(t('editor.toasts.added', { count: insertedBlocks.length }));
  };

  // Save via API Gateway & Wasm Hooks
  const handleSave = async (publish = false) => {
    if (!title.trim()) {
      showToast(t('editor.toasts.titleRequired'), 'error');
      return;
    }
    if (!slug.trim()) {
      showToast(t('editor.toasts.slugRequired'), 'error');
      return;
    }
    const scheduledAt = status === 'scheduled' ? new Date(publishAt) : null;
    if (scheduledAt && Number.isNaN(scheduledAt.getTime())) {
      showToast(t('editor.toasts.publishAtRequired'), 'error');
      return;
    }
    const unpublishDate =
      (status === 'published' || status === 'scheduled') && unpublishAt
        ? new Date(unpublishAt)
        : null;
    if (unpublishDate && Number.isNaN(unpublishDate.getTime())) {
      showToast(t('editor.toasts.unpublishAtInvalid'), 'error');
      return;
    }
    if (unpublishDate) {
      const compareDate = scheduledAt || new Date();
      if (unpublishDate.getTime() <= compareDate.getTime()) {
        showToast(t('editor.toasts.unpublishBeforePublish'), 'error');
        return;
      }
    }

    setIsSaving(true);
    try {
      const payload = {
        title,
        slug,
        status,
        publish,
        // The server publishes scheduled content at this time (a past time publishes it now)
        ...(scheduledAt ? { published_at: scheduledAt.toISOString() } : {}),
        ...(unpublishDate ? { unpublish_at: unpublishDate.toISOString() } : { unpublish_at: null }),
        blocks,
        // author_id is omitted: new content records the logged-in user, updates keep the author
        // Omit the relation entirely when no category is selected
        ...(contentType === 'article' && categoryId ? { category_id: categoryId } : {}),
        ...(contentType === 'page' ? { layout } : {}),
      };
      // Content opened from the list is updated in place; otherwise a new item is created
      const saved = selectedId
        ? await aegisApi.updateContent(contentType, selectedId, payload)
        : await aegisApi.createContent(contentType, payload);

      showToast(t('editor.toasts.saved', { id: saved.id.slice(0, 8) }));
      if (tagsEnabled) {
        // The tags are saved with the content (they have their own endpoint)
        try {
          await aegisApi.setContentTags(
            contentType,
            saved.id,
            contentTags.map((tag) => tag.id)
          );
        } catch (err: any) {
          showToast(err.message || t('tags.saveFailedForContent'), 'error');
        }
      }
      loadContents();
      // Show what was stored: plugins may change the title, slug and blocks on save
      handleSelectContent(saved);
    } catch (err: any) {
      showToast(err.message || t('editor.toasts.saveFailed'), 'error');
    } finally {
      setIsSaving(false);
    }
  };

  const handleToggleCommentsOpen = async () => {
    if (!selectedId || togglingComments) return;
    const nextOpen = !commentsOpen;
    setTogglingComments(true);
    try {
      const res = await aegisApi.setContentCommentsOpen(contentType, selectedId, nextOpen);
      setCommentsOpen(res.open);
      showToast(
        t(res.open ? 'comments.commentsOpenedToast' : 'comments.commentsClosedToast', {
          title: title.trim() || slug,
        })
      );
    } catch (err: any) {
      showToast(err.message || t('api.contentCommentsFailed'), 'error');
    } finally {
      setTogglingComments(false);
    }
  };

  const handleOpenContentFromSchedule = async (type: string, id: string) => {
    try {
      const item = await aegisApi.getContent(type, id);
      handleSelectContent(item);
      setView('editor');
    } catch (err: any) {
      showToast(err.message || t('schedule.loadFailed'), 'error');
    }
  };

  if (session === undefined) {
    return <SessionCheckingScreen />;
  }
  if (session === null) {
    return (
      <LoginView
        onLoggedIn={handleLoggedIn}
        notice={
          loginNotice === 'unreachable'
            ? t('common.unreachable')
            : loginNotice
              ? t(`login.notices.${loginNotice}`)
              : null
        }
        initialError={ssoError}
        verifyToken={verifyToken}
        resetToken={resetToken}
        inviteToken={inviteToken}
      />
    );
  }

  const editable = canEdit(session);
  const showShop = editable && shopEnabled;
  const showComments = editable && commentsEnabled;
  const showForum = editable && forumEnabled;
  const showContact = editable && contactForms.length > 0;

  return (
    <div className={styles.app}>
      <Header
        view={view}
        onViewChange={setView}
        isOnline={isOnline}
        onSave={() => handleSave()}
        onPublish={() => handleSave(true)}
        isSaving={isSaving}
        session={session}
        canSave={editable}
        canPublish={editable && Boolean(selectedId) && status === 'published'}
        showShop={showShop}
        showComments={showComments}
        showForum={showForum}
        showContact={showContact}
        showAnalytics={analyticsEnabled}
        profile={profile}
        onOpenProfile={() =>
          profile
            ? setIsProfileOpen(true)
            : showToast(t('editor.toasts.profileUnavailable'), 'error')
        }
        onLogout={handleLogout}
      />

      {view === 'schedule' ? (
        <ScheduleCalendar onOpenContent={handleOpenContentFromSchedule} onShowToast={showToast} />
      ) : view === 'products' && showShop ? (
        <ProductManager categories={categories} tagsEnabled={tagsEnabled} onShowToast={showToast} />
      ) : view === 'orders' && showShop ? (
        <OrderManager onShowToast={showToast} />
      ) : view === 'comments' && showComments ? (
        <CommentManager isAdmin={isAdmin(session)} onShowToast={showToast} />
      ) : view === 'forum' && showForum ? (
        <ForumManager isAdmin={isAdmin(session)} onShowToast={showToast} />
      ) : view === 'contact' && showContact ? (
        <ContactInbox onShowToast={showToast} />
      ) : view === 'analytics' && analyticsEnabled ? (
        <AnalyticsView onShowToast={showToast} />
      ) : view === 'settings' ? (
        <SettingsView
          session={session}
          section={settingsSection}
          onSectionChange={setSettingsSection}
          categories={categories}
          onCategoryCreated={handleCategoryCreated}
          tagsEnabled={tagsEnabled}
          onSelfUpdated={setProfile}
          onShowToast={showToast}
        />
      ) : (
        <div className={styles.workspace}>
          {/* Mobile Pane Switcher Bar */}
          <div
            className={styles.mobilePaneBar}
            role="tablist"
            aria-label={t('editor.mobilePanes.aria')}
          >
            <button
              type="button"
              role="tab"
              aria-selected={mobilePane === 'list'}
              className={cx(
                styles.mobilePaneTab,
                mobilePane === 'list' && styles.mobilePaneTabActive
              )}
              onClick={() => setMobilePane('list')}
            >
              <List size={14} />
              <span>{t('editor.mobilePanes.list')}</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mobilePane === 'editor'}
              className={cx(
                styles.mobilePaneTab,
                mobilePane === 'editor' && styles.mobilePaneTabActive
              )}
              onClick={() => setMobilePane('editor')}
            >
              <PenSquare size={14} />
              <span>{t('editor.mobilePanes.editor')}</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mobilePane === 'preview'}
              className={cx(
                styles.mobilePaneTab,
                mobilePane === 'preview' && styles.mobilePaneTabActive
              )}
              onClick={() => setMobilePane('preview')}
            >
              <Eye size={14} />
              <span>{t('editor.mobilePanes.preview')}</span>
            </button>
          </div>

          <div className={styles.workspaceColumns}>
            {/* Sidebar */}
            <div className={cx(styles.paneList, mobilePane !== 'list' && styles.paneHiddenMobile)}>
              <ContentList
                contents={contents}
                total={contentsTotal}
                page={listPage}
                pageSize={LIST_PAGE_SIZE}
                loading={contentsLoading}
                filters={listFilters}
                onFiltersChange={handleFiltersChange}
                onPageChange={handlePageChange}
                categories={categories}
                tags={tags}
                selectedId={selectedId}
                onSelect={handleSelectSummary}
                onNew={handleNew}
                contentType={contentType}
                onContentTypeChange={handleContentTypeChange}
              />
            </div>

            {/* Main Editor Center Column */}
            <main
              className={cx(styles.editorMain, mobilePane === 'list' && styles.paneHiddenMobile)}
            >
              <div
                className={cx(
                  styles.editorColumn,
                  mobilePane !== 'editor' && styles.paneHiddenMobile
                )}
              >
                {/* Meta Section */}
                <div className={styles.metaSection}>
                  <input
                    type="text"
                    className={styles.titleInput}
                    placeholder={t('editor.titlePlaceholder')}
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
                    <Field label={t('editor.slug')}>
                      <input
                        type="text"
                        value={slug}
                        onChange={(e) => setSlug(e.target.value)}
                        placeholder="post-slug"
                      />
                    </Field>

                    <Field label={t('editor.status')} className={styles.statusField}>
                      <select
                        value={status}
                        onChange={(e) => setStatus(e.target.value as EditorStatus)}
                      >
                        <option value="draft">{t('common.status.draft')}</option>
                        <option value="scheduled">{t('common.status.scheduled')}</option>
                        <option value="published">{t('common.status.published')}</option>
                      </select>
                    </Field>

                    {status === 'scheduled' && (
                      <Field label={t('editor.publishAt')}>
                        <input
                          type="datetime-local"
                          value={publishAt}
                          onChange={(e) => setPublishAt(e.target.value)}
                          aria-label={t('editor.publishAtAria')}
                        />
                      </Field>
                    )}

                    {(status === 'published' || status === 'scheduled') && (
                      <Field label={t('editor.unpublishAt')}>
                        <input
                          type="datetime-local"
                          value={unpublishAt}
                          onChange={(e) => setUnpublishAt(e.target.value)}
                          aria-label={t('editor.unpublishAtAria')}
                        />
                      </Field>
                    )}
                  </FieldRow>

                  {contentType === 'article' ? (
                    <FieldRow>
                      <Field label={t('editor.category')}>
                        <select
                          value={categoryId}
                          onChange={(e) => setCategoryId(e.target.value)}
                          aria-label={t('editor.categoryAria')}
                        >
                          <option value="">{t('editor.noCategory')}</option>
                          {categories.map((category) => (
                            <option key={category.id} value={category.id}>
                              {category.title}
                            </option>
                          ))}
                          {categoryId && !categories.some((c) => c.id === categoryId) && (
                            <option value={categoryId}>
                              {t('editor.unknownCategory', { id: categoryId.slice(0, 8) })}
                            </option>
                          )}
                        </select>
                      </Field>
                      <Button
                        variant="tool"
                        onClick={() => {
                          setSettingsSection('categories');
                          setView('settings');
                        }}
                        title={t('editor.manageCategoriesTitle')}
                      >
                        <Tags size={14} />
                        <span>{t('editor.manageCategories')}</span>
                      </Button>
                      {showComments && selectedId && commentContentTypes.includes(contentType) && (
                        <Button
                          variant="tool"
                          disabled={togglingComments}
                          onClick={handleToggleCommentsOpen}
                          title={t(
                            commentsOpen ? 'comments.closeComments' : 'comments.openComments'
                          )}
                          aria-label={t(
                            commentsOpen ? 'comments.closeComments' : 'comments.openComments'
                          )}
                        >
                          {commentsOpen ? (
                            <MessageSquare size={14} />
                          ) : (
                            <MessageSquareOff size={14} />
                          )}
                          <span>
                            {t(
                              commentsOpen ? 'editor.commentsAccepting' : 'editor.commentsStopped'
                            )}
                          </span>
                        </Button>
                      )}
                    </FieldRow>
                  ) : (
                    <FieldRow>
                      <Field label={t('editor.layout')}>
                        <select
                          value={layout}
                          onChange={(e) => setLayout(e.target.value as 'default' | 'blank')}
                          aria-label={t('editor.layoutAria')}
                        >
                          <option value="default">{t('editor.layoutDefault')}</option>
                          <option value="blank">{t('editor.layoutBlank')}</option>
                        </select>
                      </Field>
                      {showComments && selectedId && commentContentTypes.includes(contentType) && (
                        <Button
                          variant="tool"
                          disabled={togglingComments}
                          onClick={handleToggleCommentsOpen}
                          title={t(
                            commentsOpen ? 'comments.closeComments' : 'comments.openComments'
                          )}
                          aria-label={t(
                            commentsOpen ? 'comments.closeComments' : 'comments.openComments'
                          )}
                        >
                          {commentsOpen ? (
                            <MessageSquare size={14} />
                          ) : (
                            <MessageSquareOff size={14} />
                          )}
                          <span>
                            {t(
                              commentsOpen ? 'editor.commentsAccepting' : 'editor.commentsStopped'
                            )}
                          </span>
                        </Button>
                      )}
                    </FieldRow>
                  )}

                  {tagsEnabled && (
                    <FieldRow>
                      <Field label={t('tags.label')}>
                        <TagPicker
                          selected={contentTags}
                          onChange={setContentTags}
                          canCreate={editable}
                          onShowToast={showToast}
                        />
                      </Field>
                    </FieldRow>
                  )}
                </div>

                {selectedId && (
                  <ContentVersionHistory
                    versions={contentVersions}
                    loading={loadingContentVersions}
                    onRestore={async (revision) => {
                      if (!window.confirm(t('editor.restoreVersionConfirm', { revision }))) return;
                      try {
                        const restored = await aegisApi.restoreContentVersion(
                          contentType,
                          selectedId,
                          revision
                        );
                        handleSelectContent(restored);
                        showToast(t('editor.versionRestored'));
                      } catch (error) {
                        showToast(
                          error instanceof Error ? error.message : t('editor.versionRestoreFailed'),
                          'error'
                        );
                      }
                    }}
                  />
                )}

                {/* Editor Mode Selector & Paste Actions */}
                <div className={styles.editorModeBar}>
                  <div className={styles.modeTabs} role="tablist" aria-label={t('editor.mode')}>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={editorMode === 'blocks'}
                      className={cx(styles.modeTab, editorMode === 'blocks' && styles.activeMode)}
                      onClick={() => handleSwitchMode('blocks')}
                    >
                      <LayoutGrid size={14} />
                      <span>{t('editor.blocks')}</span>
                    </button>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={editorMode === 'markdown'}
                      className={cx(styles.modeTab, editorMode === 'markdown' && styles.activeMode)}
                      onClick={() => handleSwitchMode('markdown')}
                    >
                      <FileText size={14} />
                      <span>{t('editor.markdown')}</span>
                    </button>
                  </div>

                  <div className={styles.modeActions}>
                    <Button
                      variant="tool"
                      onClick={() => setIsPasteModalOpen(true)}
                      title={t('editor.pasteMarkdownTitle')}
                    >
                      <Clipboard size={14} />
                      <span>{t('editor.pasteMarkdown')}</span>
                    </Button>
                  </div>
                </div>

                {editorMode === 'markdown' ? (
                  <MarkdownEditor
                    value={markdownText}
                    onChange={handleMarkdownChange}
                    blocks={blocks}
                    onSave={() => handleSave()}
                  />
                ) : (
                  <>
                    {/* Block Insertion Toolbar */}
                    <div className={styles.blockToolbar}>
                      <span className={styles.toolbarLabel}>{t('editor.addBlock')}</span>
                      <Button
                        variant="tool"
                        onClick={() =>
                          addBlock('core/heading', t('editor.defaults.sectionTitle'), { level: 2 })
                        }
                      >
                        <Hash size={14} />
                        <span>{t('editor.heading')}</span>
                      </Button>
                      <Button variant="tool" onClick={() => addBlock('core/paragraph', '')}>
                        <AlignLeft size={14} />
                        <span>{t('editor.paragraph')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() => addBlock('core/code', '', { language: 'rust' })}
                      >
                        <Code2 size={14} />
                        <span>{t('editor.code')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() => addBlock('core/list', '', { ordered: false })}
                      >
                        <List size={14} />
                        <span>{t('editor.list')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() =>
                          // Starts empty: the image and its alt text come from an upload or the media library
                          addBlock('core/image', '', { src: '', alt: '' })
                        }
                      >
                        <ImageIcon size={14} />
                        <span>{t('editor.image')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() =>
                          addBlock('core/table', '', {
                            header: true,
                            align: [null, null],
                            rows: [
                              [
                                t('editor.defaults.column', { n: 1 }),
                                t('editor.defaults.column', { n: 2 }),
                              ],
                              ['', ''],
                              ['', ''],
                            ],
                          })
                        }
                      >
                        <Table size={14} />
                        <span>{t('editor.table')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() =>
                          addBlock('core/toc', '', {
                            title: t('editor.defaults.tocTitle'),
                            maxLevel: TOC_DEFAULT_MAX_LEVEL,
                          })
                        }
                      >
                        <ListTree size={14} />
                        <span>{t('editor.toc')}</span>
                      </Button>
                      <Button variant="tool" onClick={() => addBlock('core/quote', '')}>
                        <Quote size={14} />
                        <span>{t('editor.quote')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() => addBlock('core/callout', '', { variant: 'note' })}
                      >
                        <Info size={14} />
                        <span>{t('editor.callout')}</span>
                      </Button>
                      <Button
                        variant="tool"
                        onClick={() => addBlock('core/details', '', { summary: '', open: false })}
                      >
                        <ListCollapse size={14} />
                        <span>{t('editor.details')}</span>
                      </Button>
                      <Button variant="tool" onClick={() => addBlock('core/separator', '')}>
                        <Minus size={14} />
                        <span>{t('editor.separator')}</span>
                      </Button>
                      <Button variant="tool" onClick={() => addBlock('core/html', '')}>
                        <FileCode size={14} />
                        <span>{t('editor.html')}</span>
                      </Button>
                      {contactForms.length > 0 && (
                        <Button
                          variant="tool"
                          onClick={() =>
                            addBlock('core/contact-form', '', {
                              form: contactForms.some((f) => f.name === 'default')
                                ? 'default'
                                : contactForms[0].name,
                            })
                          }
                        >
                          <Mail size={14} />
                          <span>{t('editor.contactForm')}</span>
                        </Button>
                      )}
                      {blockDefinitions.map((definition) => (
                        <Button
                          key={definition.name}
                          variant="tool"
                          title={
                            localize(definition.description, i18n.language) ||
                            definition.plugin_display_name
                          }
                          onClick={() =>
                            addBlock(definition.name, '', defaultAttributes(definition))
                          }
                        >
                          <Puzzle size={14} />
                          <span>{localize(definition.label, i18n.language)}</span>
                        </Button>
                      ))}
                    </div>

                    {/* Block Canvas */}
                    <div className={styles.blocksList} onPaste={handleCanvasPaste}>
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
                          blockDefinitions={blockDefinitions}
                          contactForms={contactForms}
                        />
                      ))}

                      {blocks.length === 0 && (
                        <div className={styles.emptyBlocks}>
                          <p>{t('editor.noBlocks')}</p>
                          <Button
                            className={styles.addFirst}
                            onClick={() => addBlock('core/paragraph')}
                          >
                            <Plus size={16} /> {t('editor.addFirst')}
                          </Button>
                        </div>
                      )}
                    </div>
                  </>
                )}
              </div>

              {/* Right Live Preview Column */}
              <div
                className={cx(
                  styles.panePreview,
                  mobilePane !== 'preview' && styles.paneHiddenMobile
                )}
              >
                <Preview
                  contentType={contentType}
                  title={title}
                  slug={slug}
                  categoryId={categoryId}
                  layout={contentType === 'page' ? layout : undefined}
                  blocks={blocks}
                />
              </div>
            </main>
          </div>
        </div>
      )}

      {isProfileOpen && profile && (
        <ProfileModal
          user={profile}
          canChangePassword={!isSsoSession(session)}
          onUpdated={setProfile}
          onShowToast={showToast}
          onClose={() => setIsProfileOpen(false)}
        />
      )}

      {isPasteModalOpen && (
        <PasteMarkdownModal
          onInsert={handlePasteInsert}
          onClose={() => setIsPasteModalOpen(false)}
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
