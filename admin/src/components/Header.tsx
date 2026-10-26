import React, { useState, useEffect } from 'react';
import {
  Activity,
  Send,
  PenSquare,
  LogOut,
  Settings,
  Package,
  ShoppingBag,
  MessageSquare,
  Calendar,
  Menu,
  X,
  User,
  MessagesSquare,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { SessionInfo, UserAccount } from '../types';
import { cx } from '../utils/cx';
import { displayNameOf } from '../utils/roles';
import { Avatar } from './ui/Avatar';
import { BrandMark } from './ui/BrandMark';
import { Button } from './ui/Button';
import { LanguageSelect } from './ui/LanguageSelect';
import { RoleLabel } from './ui/RoleLabel';
import styles from './Header.module.css';

const ROLE_BADGE: Record<string, string | undefined> = {
  admin: styles.roleAdmin,
  editor: styles.roleEditor,
};

export type AppView =
  'editor' | 'schedule' | 'products' | 'orders' | 'comments' | 'forum' | 'settings';

interface HeaderProps {
  view: AppView;
  onViewChange: (view: AppView) => void;
  isOnline: boolean;
  onSave: () => void;
  onPublish?: () => void;
  isSaving: boolean;
  session: SessionInfo;
  /** False for viewers: saving needs the editor role */
  canSave: boolean;
  canPublish?: boolean;
  /** Products / Orders tabs: the shop is enabled (`[commerce].enabled`) and the user can edit */
  showShop: boolean;
  /** Comments tab: comments are enabled (`[comments].enabled`) and the user can edit */
  showComments: boolean;
  /** Forum tab: forum is enabled (`[forum].enabled`) and the user can edit */
  showForum: boolean;
  /** The signed-in user's account (avatar, current display name), once loaded */
  profile: UserAccount | null;
  onOpenProfile: () => void;
  onLogout: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  view,
  onViewChange,
  isOnline,
  onSave,
  onPublish,
  isSaving,
  session,
  canSave,
  canPublish = false,
  showShop,
  showComments,
  showForum,
  profile,
  onOpenProfile,
  onLogout,
}) => {
  const { t } = useTranslation();
  const [isMenuOpen, setIsMenuOpen] = useState(false);

  // Close menu with Escape key
  useEffect(() => {
    if (!isMenuOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setIsMenuOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isMenuOpen]);

  // The account reflects profile edits right away; the session keeps the name from sign-in
  const name = profile?.display_name || displayNameOf(session);

  const navItems: Array<{ id: AppView; label: string; icon: React.ReactNode }> = [
    { id: 'editor', label: t('header.tabs.editor'), icon: <PenSquare size={15} /> },
    { id: 'schedule', label: t('header.tabs.schedule'), icon: <Calendar size={15} /> },
    ...(showShop
      ? [
          {
            id: 'products' as const,
            label: t('header.tabs.products'),
            icon: <Package size={15} />,
          },
          {
            id: 'orders' as const,
            label: t('header.tabs.orders'),
            icon: <ShoppingBag size={15} />,
          },
        ]
      : []),
    ...(showComments
      ? [
          {
            id: 'comments' as const,
            label: t('header.tabs.comments'),
            icon: <MessageSquare size={15} />,
          },
        ]
      : []),
    ...(showForum
      ? [
          {
            id: 'forum' as const,
            label: t('header.tabs.forum'),
            icon: <MessagesSquare size={15} />,
          },
        ]
      : []),
    { id: 'settings', label: t('header.tabs.settings'), icon: <Settings size={15} /> },
  ];

  const handleSelectView = (targetView: AppView) => {
    onViewChange(targetView);
    setIsMenuOpen(false);
  };

  const handleProfileClick = () => {
    setIsMenuOpen(false);
    onOpenProfile();
  };

  const handleLogoutClick = () => {
    setIsMenuOpen(false);
    onLogout();
  };

  return (
    <header className={styles.header}>
      <div className={styles.left}>
        <div className={styles.logo}>
          <BrandMark />
          <span>AegisCMS</span>
        </div>
        <span className={styles.badge}>{t('header.badge')}</span>
        <nav className={styles.tabs} aria-label={t('header.views')}>
          {navItems.map((item) => (
            <button
              key={item.id}
              className={cx(styles.tab, view === item.id && styles.active)}
              onClick={() => handleSelectView(item.id)}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </nav>
      </div>

      <div className={styles.right}>
        <div
          className={styles.health}
          title={t('header.gateway', {
            state: isOnline ? t('header.connected') : t('header.offline'),
          })}
        >
          <span className={cx(styles.statusDot, isOnline && styles.online)} />
          <span className={styles.healthLabel}>
            {t('header.gateway', {
              state: isOnline ? t('header.connected') : t('header.offline'),
            })}
          </span>
        </div>

        {view === 'editor' && (
          <>
            {canPublish && onPublish && (
              <Button
                variant="tool"
                onClick={onPublish}
                disabled={isSaving}
                title={t('header.publishTitle')}
                className={styles.saveButton}
              >
                <Send size={16} />
                <span className={styles.saveButtonLabel}>{t('header.publish')}</span>
              </Button>
            )}
            <Button
              variant="primary"
              onClick={onSave}
              disabled={isSaving || !canSave}
              title={canSave ? t('header.saveTitle') : t('header.saveNeedsEditor')}
              className={styles.saveButton}
            >
              {isSaving ? <Activity className="animate-spin" size={16} /> : <Send size={16} />}
              <span className={styles.saveButtonLabel}>
                {isSaving ? t('header.saving') : t('header.save')}
              </span>
            </Button>
          </>
        )}

        <div className={styles.desktopControls}>
          <LanguageSelect />

          <div className={styles.userMenu}>
            <button
              type="button"
              className={styles.profileButton}
              onClick={onOpenProfile}
              title={t('header.profileTitle', {
                who: profile?.email || session.metadata?.email || session.user_id,
              })}
            >
              <Avatar url={profile?.avatar_url} name={name} size={26} />
              <span className={styles.userName}>{name}</span>
            </button>
            <span className={cx(styles.roleBadge, ROLE_BADGE[session.role])}>
              <RoleLabel role={session.role} />
            </span>
            <Button
              variant="icon"
              onClick={onLogout}
              aria-label={t('header.signOut')}
              title={t('header.signOut')}
            >
              <LogOut size={14} />
            </Button>
          </div>
        </div>

        {/* Mobile menu toggle */}
        <button
          type="button"
          className={styles.menuButton}
          onClick={() => setIsMenuOpen((prev) => !prev)}
          aria-label={t('header.toggleMenu')}
          aria-expanded={isMenuOpen}
        >
          {isMenuOpen ? <X size={20} /> : <Menu size={20} />}
        </button>
      </div>

      {/* Mobile Drawer / Dropdown */}
      {isMenuOpen && (
        <>
          <div
            className={styles.menuBackdrop}
            onClick={() => setIsMenuOpen(false)}
            aria-hidden="true"
          />
          <div className={styles.mobileMenu} role="dialog" aria-modal="true">
            <nav className={styles.mobileNav} aria-label={t('header.views')}>
              {navItems.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={cx(styles.mobileNavItem, view === item.id && styles.mobileNavActive)}
                  onClick={() => handleSelectView(item.id)}
                >
                  <span className={styles.mobileNavIcon}>{item.icon}</span>
                  <span className={styles.mobileNavLabel}>{item.label}</span>
                </button>
              ))}
            </nav>

            <div className={styles.mobileDivider} />

            <div className={styles.mobileExtras}>
              <div
                className={styles.mobileHealth}
                title={t('header.gateway', {
                  state: isOnline ? t('header.connected') : t('header.offline'),
                })}
              >
                <span className={cx(styles.statusDot, isOnline && styles.online)} />
                <span>
                  {t('header.gateway', {
                    state: isOnline ? t('header.connected') : t('header.offline'),
                  })}
                </span>
              </div>
              <div className={styles.mobileLang}>
                <LanguageSelect />
              </div>
            </div>

            <div className={styles.mobileDivider} />

            <div className={styles.mobileUserSection}>
              <div className={styles.mobileUserInfo}>
                <Avatar url={profile?.avatar_url} name={name} size={36} />
                <div className={styles.mobileUserDetails}>
                  <span className={styles.mobileUserName}>{name}</span>
                  <span className={cx(styles.roleBadge, ROLE_BADGE[session.role])}>
                    <RoleLabel role={session.role} />
                  </span>
                </div>
              </div>
              <div className={styles.mobileUserActions}>
                <Button
                  variant="tool"
                  onClick={handleProfileClick}
                  className={styles.mobileUserActionBtn}
                >
                  <User size={14} />
                  <span>{t('header.profileTitle', { who: '' }).replace(' — ', '').trim()}</span>
                </Button>
                <Button
                  variant="tool"
                  onClick={handleLogoutClick}
                  className={styles.mobileUserActionBtn}
                >
                  <LogOut size={14} />
                  <span>{t('header.signOut')}</span>
                </Button>
              </div>
            </div>
          </div>
        </>
      )}
    </header>
  );
};
