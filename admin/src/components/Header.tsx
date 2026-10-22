import React from 'react';
import {
  ShieldCheck,
  Activity,
  Send,
  PenSquare,
  ScrollText,
  LogOut,
  UserRound,
} from 'lucide-react';
import type { SessionInfo } from '../types';
import { displayNameOf } from '../utils/roles';

export type AppView = 'editor' | 'audit';

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
    <header className="app-header">
      <div className="header-left">
        <div className="brand-logo">
          <div className="brand-icon">
            <ShieldCheck size={20} color="#fff" />
          </div>
          <span>AegisCMS</span>
        </div>
        <span className="brand-badge">Zero-Trust Sandbox</span>
        <nav className="view-tabs" aria-label="Views">
          <button
            className={`view-tab ${view === 'editor' ? 'active' : ''}`}
            onClick={() => onViewChange('editor')}
          >
            <PenSquare size={14} />
            <span>Editor</span>
          </button>
          {canViewAudit && (
            <button
              className={`view-tab ${view === 'audit' ? 'active' : ''}`}
              onClick={() => onViewChange('audit')}
            >
              <ScrollText size={14} />
              <span>Audit Logs</span>
            </button>
          )}
        </nav>
      </div>

      <div className="header-right">
        <div className="health-status">
          <span className={`status-dot ${isOnline ? 'online' : ''}`} />
          <span>Core Gateway: {isOnline ? 'Connected' : 'Offline'}</span>
        </div>

        {view === 'editor' && (
          <button
            className="btn btn-primary"
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
          </button>
        )}

        <div className="user-menu" title={session.metadata?.email || session.user_id}>
          <UserRound size={14} />
          <span className="user-name">{displayNameOf(session)}</span>
          <span className={`role-badge role-${session.role}`}>{session.role}</span>
          <button className="icon-btn" onClick={onLogout} aria-label="Sign out" title="Sign out">
            <LogOut size={14} />
          </button>
        </div>
      </div>
    </header>
  );
};
