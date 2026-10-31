import React from 'react';
import {
  Globe,
  Palette,
  Blocks,
  Tags,
  Tag as TagIcon,
  Users,
  ScrollText,
  Search,
  Bell,
  ShieldCheck,
  SlidersHorizontal,
  ArrowRightLeft,
  Mail,
  Unlink,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ContentItem, SessionInfo, UserAccount, ContentSummary } from '../types';
import { canEdit, isAdmin } from '../utils/roles';
import { cx } from '../utils/cx';
import { AuditLogView } from './AuditLogView';
import { CategoryManager } from './CategoryManager';
import { HeaderManager } from './HeaderManager';
import { NewsletterManager } from './NewsletterManager';
import { NotificationManager } from './NotificationManager';
import { PluginManager } from './PluginManager';
import { RedirectManager } from './RedirectManager';
import { SeoManager } from './SeoManager';
import { SiteManager } from './SiteManager';
import { SystemManager } from './SystemManager';
import { TagManager } from './TagManager';
import { ThemeManager } from './ThemeManager';
import { UserManager } from './UserManager';
import { BrokenLinksManager } from './BrokenLinksManager';
import styles from './SettingsView.module.css';

export type SettingsSection =
  | 'site'
  | 'system'
  | 'seo'
  | 'headers'
  | 'redirects'
  | 'broken-links'
  | 'newsletter'
  | 'notifications'
  | 'themes'
  | 'plugins'
  | 'categories'
  | 'tags'
  | 'users'
  | 'audit';

const SECTIONS: Array<{
  id: SettingsSection;
  icon: React.ReactNode;
  /** Who sees the section (everyone when omitted) */
  visible?: (session: SessionInfo) => boolean;
}> = [
  { id: 'site', icon: <Globe size={16} />, visible: isAdmin },
  { id: 'system', icon: <SlidersHorizontal size={16} />, visible: isAdmin },
  { id: 'seo', icon: <Search size={16} />, visible: isAdmin },
  { id: 'headers', icon: <ShieldCheck size={16} />, visible: isAdmin },
  { id: 'redirects', icon: <ArrowRightLeft size={16} />, visible: isAdmin },
  { id: 'broken-links', icon: <Unlink size={16} />, visible: isAdmin },
  { id: 'newsletter', icon: <Mail size={16} />, visible: isAdmin },
  { id: 'notifications', icon: <Bell size={16} />, visible: isAdmin },
  { id: 'themes', icon: <Palette size={16} /> },
  { id: 'plugins', icon: <Blocks size={16} /> },
  { id: 'categories', icon: <Tags size={16} /> },
  { id: 'tags', icon: <TagIcon size={16} /> },
  { id: 'users', icon: <Users size={16} />, visible: isAdmin },
  { id: 'audit', icon: <ScrollText size={16} />, visible: isAdmin },
];

interface SettingsViewProps {
  session: SessionInfo;
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  categories: ContentSummary[];
  onCategoryCreated: (category: ContentItem) => void;
  /** Whether `[tags]` is enabled on the server (the Tags section is hidden otherwise) */
  tagsEnabled: boolean;
  onSelfUpdated: (account: UserAccount) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Site-wide settings (site name and favicon, SEO, notifications, themes, plugins, categories, users, audit logs), kept apart from the editor */
export const SettingsView: React.FC<SettingsViewProps> = ({
  session,
  section,
  onSectionChange,
  categories,
  onCategoryCreated,
  tagsEnabled,
  onSelfUpdated,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const sections = SECTIONS.filter(
    (s) => (!s.visible || s.visible(session)) && (s.id !== 'tags' || tagsEnabled)
  );
  // A section the role cannot see (e.g. after signing in as another user) falls back to the first
  const current = sections.some((s) => s.id === section) ? section : sections[0].id;

  return (
    <div className={styles.settings}>
      <nav className={styles.nav} aria-label={t('settings.sectionsAria')}>
        <div className={styles.navTitle}>{t('settings.title')}</div>
        {sections.map((s) => (
          <button
            key={s.id}
            type="button"
            className={cx(styles.navItem, current === s.id && styles.active)}
            aria-current={current === s.id ? 'page' : undefined}
            onClick={() => onSectionChange(s.id)}
          >
            {s.icon}
            <span>{t(`settings.sections.${s.id}`)}</span>
          </button>
        ))}
      </nav>

      <div className={styles.content}>
        {current === 'site' && <SiteManager onShowToast={onShowToast} />}
        {current === 'system' && <SystemManager onShowToast={onShowToast} />}
        {current === 'seo' && <SeoManager onShowToast={onShowToast} />}
        {current === 'headers' && <HeaderManager onShowToast={onShowToast} />}
        {current === 'redirects' && <RedirectManager onShowToast={onShowToast} />}
        {current === 'broken-links' && <BrokenLinksManager onShowToast={onShowToast} />}
        {current === 'newsletter' && <NewsletterManager onShowToast={onShowToast} />}
        {current === 'notifications' && <NotificationManager onShowToast={onShowToast} />}
        {current === 'themes' && <ThemeManager session={session} onShowToast={onShowToast} />}
        {current === 'plugins' && <PluginManager session={session} onShowToast={onShowToast} />}
        {current === 'categories' && (
          <CategoryManager
            categories={categories}
            canCreate={canEdit(session)}
            onCreated={onCategoryCreated}
          />
        )}
        {current === 'tags' && <TagManager canEdit={canEdit(session)} onShowToast={onShowToast} />}
        {current === 'users' && (
          <UserManager session={session} onSelfUpdated={onSelfUpdated} onShowToast={onShowToast} />
        )}
        {current === 'audit' && <AuditLogView />}
      </div>
    </div>
  );
};
