import React, { useState, useEffect } from 'react';
import {
  Palette,
  CheckCircle,
  ExternalLink,
  Sparkles,
  ShoppingBag,
  UploadCloud,
  Search,
  User,
  ShieldCheck,
  RefreshCw,
} from 'lucide-react';
import { aegisApi, SERVER_URL } from '../api/client';
import type { SessionInfo, ThemeInfo } from '../types';
import { isAdmin } from '../utils/roles';
import { cx } from '../utils/cx';
import styles from './ThemeManager.module.css';

interface ThemeManagerProps {
  session: SessionInfo;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

export const ThemeManager: React.FC<ThemeManagerProps> = ({ session, onShowToast }) => {
  const [activeTab, setActiveTab] = useState<'installed' | 'market'>('installed');
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [reloadTrigger, setReloadTrigger] = useState<number>(0);
  const [switchingThemeId, setSwitchingThemeId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [selectedCategory, setSelectedCategory] = useState<string>('All');

  const canSwitchTheme = isAdmin(session);

  useEffect(() => {
    let cancelled = false;
    aegisApi
      .listThemes()
      .then((data) => {
        if (!cancelled) {
          setThemes(data);
          setIsLoading(false);
        }
      })
      .catch((err: unknown) => {
        console.error('Failed to load themes:', err);
        if (!cancelled) {
          onShowToast('Failed to load installed themes', 'error');
          setIsLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [reloadTrigger, onShowToast]);

  const reloadThemes = () => {
    setIsLoading(true);
    setReloadTrigger((n) => n + 1);
  };

  const handleSwitchTheme = async (themeId: string) => {
    if (!canSwitchTheme) {
      onShowToast('Admin role is required to switch the active theme', 'error');
      return;
    }

    setSwitchingThemeId(themeId);
    try {
      const updated = await aegisApi.switchActiveTheme(themeId);
      onShowToast(`Activated "${updated.name}" successfully!`, 'success');
      reloadThemes();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Failed to switch theme';
      onShowToast(msg, 'error');
    } finally {
      setSwitchingThemeId(null);
    }
  };

  const authorName = (author: ThemeInfo['author']): string => {
    if (typeof author === 'string') return author;
    return author?.name || 'Aegis Community';
  };

  const authorUrl = (author: ThemeInfo['author']): string | undefined => {
    if (typeof author === 'object' && author?.url) return author.url;
    return undefined;
  };

  const filteredThemes = themes.filter((t) => {
    const q = searchQuery.toLowerCase();
    const matchesSearch =
      t.name.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q) ||
      t.tags.some((tag) => tag.toLowerCase().includes(q));
    return matchesSearch;
  });

  return (
    <main className={styles.main}>
      <div className={styles.container}>
        <div className={styles.header}>
          <div className={styles.titleRow}>
            <h1 className={styles.title}>
              <Palette className={styles.titleIcon} size={28} />
              <span>Theme Management</span>
            </h1>

            <div style={{ display: 'flex', gap: '0.75rem' }}>
              <a
                href={`${SERVER_URL}/`}
                target="_blank"
                rel="noopener noreferrer"
                className={styles.previewBtn}
                title="Open public website in a new tab"
              >
                <span>View Public Site</span>
                <ExternalLink size={14} />
              </a>
              <button
                onClick={reloadThemes}
                disabled={isLoading}
                className={styles.previewBtn}
                title="Refresh theme list"
              >
                <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
                <span>Refresh</span>
              </button>
            </div>
          </div>

          <p className={styles.description}>
            Customize the visual presentation and streaming layouts of your public site. Themes are
            sandboxed packages that compile with MiniJinja with zero performance penalty.
          </p>

          <div className={styles.tabs}>
            <button
              className={cx(styles.tab, activeTab === 'installed' && styles.tabActive)}
              onClick={() => setActiveTab('installed')}
            >
              <Sparkles size={16} />
              <span>Installed Themes</span>
              <span className={styles.tabBadge}>{themes.length}</span>
            </button>
            <button
              className={cx(styles.tab, activeTab === 'market' && styles.tabActive)}
              onClick={() => setActiveTab('market')}
            >
              <ShoppingBag size={16} />
              <span>Theme Marketplace</span>
              <span className={styles.tabBadge}>Ecosystem</span>
            </button>
          </div>
        </div>

        {activeTab === 'installed' && (
          <>
            <div className={styles.grid}>
              {filteredThemes.map((theme) => {
                const isCurrent = theme.is_active;
                const isSwitching = switchingThemeId === theme.id;
                const previewSrc = theme.preview_url
                  ? `${SERVER_URL}${theme.preview_url}`
                  : undefined;

                return (
                  <div key={theme.id} className={cx(styles.card, isCurrent && styles.cardActive)}>
                    <div className={styles.previewWrapper}>
                      {previewSrc ? (
                        <img src={previewSrc} alt={theme.name} className={styles.previewImage} />
                      ) : (
                        <div className={styles.previewFallback}>
                          <Palette size={48} opacity={0.4} />
                          <span>{theme.name}</span>
                        </div>
                      )}
                      {isCurrent && (
                        <div className={styles.activeBadge}>
                          <CheckCircle size={14} />
                          <span>Active Theme</span>
                        </div>
                      )}
                    </div>

                    <div className={styles.cardBody}>
                      <div className={styles.cardHeader}>
                        <h2 className={styles.themeName}>{theme.name}</h2>
                        <span className={styles.version}>v{theme.version}</span>
                      </div>

                      <p className={styles.themeDesc}>{theme.description}</p>

                      {theme.tags.length > 0 && (
                        <div className={styles.tags}>
                          {theme.tags.map((tag) => (
                            <span key={tag} className={styles.tag}>
                              #{tag}
                            </span>
                          ))}
                        </div>
                      )}

                      <div className={styles.metaRow}>
                        <div className={styles.author}>
                          <User size={13} />
                          {authorUrl(theme.author) ? (
                            <a
                              href={authorUrl(theme.author)}
                              target="_blank"
                              rel="noopener noreferrer"
                            >
                              {authorName(theme.author)}
                            </a>
                          ) : (
                            <span>{authorName(theme.author)}</span>
                          )}
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '0.3rem' }}>
                          <ShieldCheck size={13} />
                          <span>{theme.license || 'MIT'}</span>
                        </div>
                      </div>

                      <div className={styles.cardActions}>
                        {isCurrent ? (
                          <button className={cx(styles.activateBtn, styles.activeBtn)} disabled>
                            <CheckCircle size={16} />
                            <span>Currently Active</span>
                          </button>
                        ) : (
                          <button
                            className={styles.activateBtn}
                            onClick={() => handleSwitchTheme(theme.id)}
                            disabled={isSwitching || !canSwitchTheme}
                            title={
                              canSwitchTheme
                                ? 'Activate this theme for the public site'
                                : 'Admin role is required to switch theme'
                            }
                          >
                            {isSwitching ? (
                              <>
                                <RefreshCw size={16} className="animate-spin" />
                                <span>Activating...</span>
                              </>
                            ) : (
                              <>
                                <Palette size={16} />
                                <span>Activate Theme</span>
                              </>
                            )}
                          </button>
                        )}

                        <a
                          href={`${SERVER_URL}/`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className={styles.previewBtn}
                          title="Preview site layout"
                        >
                          <ExternalLink size={16} />
                        </a>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </>
        )}

        {activeTab === 'market' && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '2rem' }}>
            <div className={styles.marketHero}>
              <div className={styles.marketHeroHeader}>
                <div>
                  <h2 className={styles.marketTitle}>Aegis Theme Marketplace</h2>
                  <p
                    style={{ color: 'var(--text-muted)', fontSize: '0.95rem', marginTop: '0.4rem' }}
                  >
                    Discover, install, and preview community-verified themes directly from the
                    decentralized Aegis registry.
                  </p>
                </div>
                <span className={styles.marketBadge}>Marketplace v1 Preview</span>
              </div>

              <div className={styles.marketFilters}>
                <div style={{ position: 'relative', flex: 1, minWidth: '260px' }}>
                  <Search
                    size={16}
                    style={{
                      position: 'absolute',
                      left: '12px',
                      top: '50%',
                      transform: 'translateY(-50%)',
                      color: 'var(--text-dim)',
                    }}
                  />
                  <input
                    type="text"
                    placeholder="Search community themes by keyword or author..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className={styles.searchBox}
                    style={{ paddingLeft: '2.5rem' }}
                  />
                </div>

                <div className={styles.categoryFilter}>
                  {['All', 'Cyberpunk', 'Editorial', 'Dark Mode', 'Luxury', 'Portfolio'].map(
                    (cat) => (
                      <button
                        key={cat}
                        className={cx(
                          styles.categoryPill,
                          selectedCategory === cat && styles.categoryPillActive
                        )}
                        onClick={() => setSelectedCategory(cat)}
                      >
                        {cat}
                      </button>
                    )
                  )}
                </div>
              </div>
            </div>

            {/* Direct Package Import Box */}
            <div className={styles.importBox}>
              <UploadCloud size={44} className={styles.importIcon} />
              <div className={styles.importTitle}>Install Theme from Package or Registry</div>
              <p className={styles.importDesc}>
                AegisCMS themes are fully isolated directory packages containing a{' '}
                <code>theme.json</code> manifest and <code>templates/</code>. Simply drop or unpack
                any compatible theme into your server's <code>themes/</code> directory to make it
                available immediately.
              </p>
              <div style={{ display: 'flex', gap: '1rem', marginTop: '0.5rem' }}>
                <button
                  className={styles.activateBtn}
                  onClick={() =>
                    onShowToast(
                      'Direct ZIP upload installer will be available in Marketplace v2! For now, extract ZIP packages to themes/ directory.',
                      'success'
                    )
                  }
                >
                  <UploadCloud size={16} />
                  <span>Import Theme (.zip)</span>
                </button>
                <a
                  href="https://github.com/takumi-mr/AegisCMS/tree/main/themes"
                  target="_blank"
                  rel="noopener noreferrer"
                  className={styles.previewBtn}
                >
                  <span>Browse Theme Templates</span>
                  <ExternalLink size={14} />
                </a>
              </div>
            </div>
          </div>
        )}
      </div>
    </main>
  );
};
