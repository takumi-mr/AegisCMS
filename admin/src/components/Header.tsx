import React from 'react';
import { Activity, Send, PenSquare, ScrollText, LogOut, UserRound, Palette } from 'lucide-react';
import type { SessionInfo } from '../types';
import { cx } from '../utils/cx';
import { displayNameOf } from '../utils/roles';
import { BrandMark } from './ui/BrandMark';
import { Button } from './ui/Button';
import styles from './Header.module.css';

const ROLE_BADGE: Record<string, string | undefined> = {
  admin: styles.roleAdmin,
  editor: styles.roleEditor,
};

export type AppView = 'editor' | 'audit' | 'themes';

interface HeaderProps {
  view: AppView;
  onViewChange: (view: AppView) => void;
  isOnline: boolean;
  onSave: () => void;
  isSaving: boolean;
  session: SessionInfo;
  /** False for viewers: saving needs the editor role */
  canSave: boolean;
  /** Audit logs need the admin role */
  canViewAudit: boolean;
  onLogout: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  view,
  onViewChange,
  isOnline,
  onSave,
  isSaving,
  session,
  canSave,
  canViewAudit,
  onLogout,
}) => {
  return (
    <header className={styles.header}>
      <div className={styles.left}>
        <div className={styles.logo}>
          <BrandMark />
          <span>AegisCMS</span>
        </div>
        <span className={styles.badge}>Zero-Trust Sandbox</span>
        <nav className={styles.tabs} aria-label="Views">
          <button
            className={cx(styles.tab, view === 'editor' && styles.active)}
            onClick={() => onViewChange('editor')}
          >
            <PenSquare size={14} />
            <span>Editor</span>
          </button>
          <button
            className={cx(styles.tab, view === 'themes' && styles.active)}
            onClick={() => onViewChange('themes')}
          >
            <Palette size={14} />
            <span>Themes</span>
          </button>
          {canViewAudit && (
            <button
              className={cx(styles.tab, view === 'audit' && styles.active)}
              onClick={() => onViewChange('audit')}
            >
              <ScrollText size={14} />
              <span>Audit Logs</span>
            </button>
          )}
        </nav>
      </div>

      <div className={styles.right}>
        <div className={styles.health}>
          <span className={cx(styles.statusDot, isOnline && styles.online)} />
          <span>Core Gateway: {isOnline ? 'Connected' : 'Offline'}</span>
        </div>

        {view === 'editor' && (
          <Button
            variant="primary"
            onClick={onSave}
            disabled={isSaving || !canSave}
            title={
              canSave
                ? 'Save content and trigger Wasm guest hooks'
                : 'Saving requires the editor role'
            }
          >
            {isSaving ? <Activity className="animate-spin" size={16} /> : <Send size={16} />}
            <span>{isSaving ? 'Executing Hooks...' : 'Save & Run Hooks'}</span>
          </Button>
        )}

        <div className={styles.userMenu} title={session.metadata?.email || session.user_id}>
          <UserRound size={14} />
          <span className={styles.userName}>{displayNameOf(session)}</span>
          <span className={cx(styles.roleBadge, ROLE_BADGE[session.role])}>{session.role}</span>
          <Button variant="icon" onClick={onLogout} aria-label="Sign out" title="Sign out">
            <LogOut size={14} />
          </Button>
        </div>
      </div>
    </header>
  );
};
