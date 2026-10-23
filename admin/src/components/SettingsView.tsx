import React from 'react';
import { Palette, Tags, Users, ScrollText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { ContentItem, SessionInfo, UserAccount } from '../types';
import { canEdit, isAdmin } from '../utils/roles';
import { cx } from '../utils/cx';
import { AuditLogView } from './AuditLogView';
import { CategoryManager } from './CategoryManager';
import { ThemeManager } from './ThemeManager';
import { UserManager } from './UserManager';
import styles from './SettingsView.module.css';

export type SettingsSection = 'themes' | 'categories' | 'users' | 'audit';

const SECTIONS: Array<{
  id: SettingsSection;
  icon: React.ReactNode;
  /** Who sees the section (everyone when omitted) */
  visible?: (session: SessionInfo) => boolean;
}> = [
  { id: 'themes', icon: <Palette size={16} /> },
  { id: 'categories', icon: <Tags size={16} /> },
  { id: 'users', icon: <Users size={16} />, visible: isAdmin },
  { id: 'audit', icon: <ScrollText size={16} />, visible: isAdmin },
];

interface SettingsViewProps {
  session: SessionInfo;
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  categories: ContentItem[];
  onCategoryCreated: (category: ContentItem) => void;
  onSelfUpdated: (account: UserAccount) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

/** Site-wide settings (themes, categories, users, audit logs), kept apart from the editor */
export const SettingsView: React.FC<SettingsViewProps> = ({
  session,
  section,
  onSectionChange,
  categories,
  onCategoryCreated,
  onSelfUpdated,
  onShowToast,
}) => {
  const { t } = useTranslation();
  const sections = SECTIONS.filter((s) => !s.visible || s.visible(session));
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
        {current === 'themes' && <ThemeManager session={session} onShowToast={onShowToast} />}
        {current === 'categories' && (
          <CategoryManager
            categories={categories}
            canCreate={canEdit(session)}
            onCreated={onCategoryCreated}
          />
        )}
        {current === 'users' && (
          <UserManager session={session} onSelfUpdated={onSelfUpdated} onShowToast={onShowToast} />
        )}
        {current === 'audit' && <AuditLogView />}
      </div>
    </div>
  );
};
